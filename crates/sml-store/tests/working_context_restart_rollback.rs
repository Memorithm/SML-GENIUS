//! CONTEXT-2 G9: bounded, deterministic SWE1/SWC2 restart-and-rollback
//! state-machine harness (test-only).
//!
//! Model: a checkpoint is written as `write tmp -> fsync tmp -> rename tmp
//! over live`. The harness enumerates every crash point of that sequence
//! (before write, after every byte prefix of the tmp write, after fsync,
//! after rename) and, separately, every crash point of a non-atomic in-place
//! overwrite. After each crash a *fresh process* restarts and restores only
//! from the durable `live` file through `restore_into_sealed`.
//!
//! Expected outcomes come from an independent table (`expected_after_crash`)
//! that is a pure function of the crash model, not of production code. The
//! corpus seeds and admission counts are frozen; expected counters follow
//! from the `CapacityBounded` contract (`admitted = n`,
//! `retained = min(n, capacity)`, `admitted = retained + evicted + lost`).
//!
//! Integrity vs rollback (explicitly distinct):
//! - SWE1 is an **unkeyed integrity check**: it detects accidental corruption
//!   (torn / truncated / flipped bytes) and nothing else.
//! - **Rollback detection is NOT implemented** in `sml-store`. An older,
//!   intact, legitimately sealed snapshot restores successfully. Detecting it
//!   requires external **trusted monotonic state** (an anchor the attacker or
//!   a stale disk cannot rewind). The test-local `TrustedAnchor` only shows
//!   what such a check would need; it is not a product feature, holds no key
//!   and makes no authentication claim.
//!
//! `NOT_MEASURED`: no performance, model-quality or context-span claim.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_envelope::{restore_into_sealed, seal_context};
use sml_store::working_context_wire::encode_context;

/// Frozen corpus: (seed, capacity, admissions in old checkpoint, in new).
const CORPUS: &[(u64, u64, u64, u64)] = &[
    (0x0947_0001, 3, 0, 2),
    (0x0947_0002, 3, 2, 5),
    (0x0947_0003, 2, 1, 4),
    (0x0947_0004, 4, 4, 9),
];

/// Deterministic SplitMix64, frozen.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Side-effect ledger: every call to `admit` made by the harness. Restore
/// paths must never append to it.
#[derive(Default)]
struct Effects {
    admits: u64,
}

fn stream(seed: u64) -> ContextStreamId {
    ContextStreamId::new(0x4739_0000_0000_0000 | (seed & 0xFFFF)).unwrap()
}

fn fresh(seed: u64, cap: u64) -> WorkingContext {
    WorkingContext::new(
        stream(seed),
        WorkingPolicy::CapacityBounded {
            capacity: cap as usize,
        },
    )
    .unwrap()
}

/// Admit segments `from+1 ..= to` deterministically from `seed`.
fn admit_range(ctx: &mut WorkingContext, fx: &mut Effects, seed: u64, from: u64, to: u64) {
    for i in from + 1..=to {
        let mut s = seed ^ i.wrapping_mul(0xA24B_AED4_963E_E407);
        let mut h = [0_u8; 32];
        for c in h.chunks_exact_mut(8) {
            c.copy_from_slice(&splitmix(&mut s).to_le_bytes());
        }
        let seg = WorkingSegment::new(
            ContextSegmentId::new(stream(seed), 1000 + i).unwrap(),
            7 * i,
            ContextContentHash::from_sha256(h),
            splitmix(&mut s),
        )
        .unwrap();
        ctx.admit(seg).unwrap();
        fx.admits += 1;
    }
}

/// Independent contract expectations for a CapacityBounded context.
fn assert_counters(ctx: &WorkingContext, cap: u64, n: u64, what: &str) {
    let st = ctx.snapshot().stats;
    assert_eq!(st.admitted, n, "{what}: admitted");
    assert_eq!(st.retained, n.min(cap), "{what}: retained");
    assert_eq!(ctx.retained_count() as u64, n.min(cap), "{what}: residents");
    assert_eq!(
        st.admitted,
        st.retained + st.evicted + st.lost,
        "{what}: conservation"
    );
}

/// In-memory durable store: one `live` file and one `tmp` file.
#[derive(Clone, Default)]
struct Disk {
    live: Option<Vec<u8>>,
    tmp: Option<Vec<u8>>,
}

/// Crash points of the atomic protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crash {
    BeforeWrite,
    TmpPrefix(usize),
    AfterFsync,
    AfterRename,
}

/// What the restarted process must observe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Old,
    New,
    /// Restore must fail; restarted process stays at its fresh state.
    Reject,
}

/// Independent expected-outcome table for the atomic protocol: the live
/// file only ever changes at rename, so anything before rename restores the
/// old checkpoint (or rejects if there was none), and after rename the new.
fn expected_after_crash(crash: Crash, had_old: bool) -> Expect {
    match crash {
        Crash::AfterRename => Expect::New,
        _ if had_old => Expect::Old,
        _ => Expect::Reject,
    }
}

fn atomic_checkpoint(disk: &mut Disk, sealed: &[u8], crash: Crash) {
    if crash == Crash::BeforeWrite {
        return;
    }
    let upto = match crash {
        Crash::TmpPrefix(k) => k,
        _ => sealed.len(),
    };
    disk.tmp = Some(sealed[..upto].to_vec());
    if matches!(crash, Crash::TmpPrefix(_)) {
        return;
    }
    // fsync(tmp) is a no-op on this in-memory disk; the crash point marks it.
    if crash == Crash::AfterFsync {
        return;
    }
    disk.live = disk.tmp.take();
}

/// Restart: a fresh process restores from `live` only. Returns the restored
/// context or the untouched fresh one, plus whether restore succeeded.
fn restart(disk: &Disk, seed: u64, cap: u64) -> (WorkingContext, bool) {
    let mut ctx = fresh(seed, cap);
    let before = encode_context(&ctx).unwrap();
    let ok = match &disk.live {
        Some(bytes) => restore_into_sealed(&mut ctx, bytes).is_ok(),
        None => false,
    };
    if !ok {
        assert_eq!(
            encode_context(&ctx).unwrap(),
            before,
            "failed restore mutated"
        );
    }
    (ctx, ok)
}

fn build(
    seed: u64,
    cap: u64,
    n_old: u64,
    n_new: u64,
) -> (WorkingContext, Vec<u8>, Vec<u8>, Effects) {
    let mut fx = Effects::default();
    let mut ctx = fresh(seed, cap);
    admit_range(&mut ctx, &mut fx, seed, 0, n_old);
    let old = seal_context(&ctx).unwrap();
    admit_range(&mut ctx, &mut fx, seed, n_old, n_new);
    let new = seal_context(&ctx).unwrap();
    assert_eq!(fx.admits, n_new);
    (ctx, old, new, fx)
}

#[test]
fn corpus_is_deterministic_and_matches_contract_counters() {
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (ctx_a, old_a, new_a, _) = build(seed, cap, n_old, n_new);
        let (_, old_b, new_b, _) = build(seed, cap, n_old, n_new);
        assert_eq!(old_a, old_b, "seed {seed:#x}: old not deterministic");
        assert_eq!(new_a, new_b, "seed {seed:#x}: new not deterministic");
        assert_ne!(old_a, new_a, "seed {seed:#x}");
        assert_counters(&ctx_a, cap, n_new, "live");
    }
}

#[test]
fn every_atomic_crash_point_restores_old_or_new_exactly() {
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (_, old, new, fx) = build(seed, cap, n_old, n_new);
        for had_old in [false, true] {
            let mut points = vec![Crash::BeforeWrite, Crash::AfterFsync, Crash::AfterRename];
            points.extend((0..=new.len()).map(Crash::TmpPrefix));
            for crash in points {
                let mut disk = Disk::default();
                if had_old {
                    disk.live = Some(old.clone());
                }
                atomic_checkpoint(&mut disk, &new, crash);
                let (ctx, ok) = restart(&disk, seed, cap);
                let what = format!("seed {seed:#x} old={had_old} {crash:?}");
                match expected_after_crash(crash, had_old) {
                    Expect::Old => {
                        assert!(ok, "{what}");
                        assert_eq!(seal_context(&ctx).unwrap(), old, "{what}");
                        assert_counters(&ctx, cap, n_old, &what);
                    }
                    Expect::New => {
                        assert!(ok, "{what}");
                        assert_eq!(seal_context(&ctx).unwrap(), new, "{what}");
                        assert_counters(&ctx, cap, n_new, &what);
                    }
                    Expect::Reject => {
                        assert!(!ok, "{what}");
                        assert_counters(&ctx, cap, 0, &what);
                    }
                }
            }
        }
        assert_eq!(fx.admits, n_new, "restore replayed admissions");
    }
}

#[test]
fn every_inplace_overwrite_crash_is_rejected_or_exact() {
    // Non-atomic overwrite of `live`: each crash leaves new[..k] ++ old[k..].
    // Integrity check must reject every torn image; only the two intact
    // images may restore, each exactly.
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (_, old, new, _) = build(seed, cap, n_old, n_new);
        let mut accepted = 0;
        for k in 0..=new.len().max(old.len()) {
            let mut img = new[..k.min(new.len())].to_vec();
            if k < old.len() {
                img.extend_from_slice(&old[k..]);
            }
            let disk = Disk {
                live: Some(img.clone()),
                tmp: None,
            };
            let (ctx, ok) = restart(&disk, seed, cap);
            let what = format!("seed {seed:#x} torn k={k}");
            if img == old {
                assert!(ok, "{what}");
                assert_counters(&ctx, cap, n_old, &what);
                accepted += 1;
            } else if img == new {
                assert!(ok, "{what}");
                assert_counters(&ctx, cap, n_new, &what);
                accepted += 1;
            } else {
                assert!(!ok, "{what}: torn image accepted");
                assert_counters(&ctx, cap, 0, &what);
            }
        }
        assert!(accepted >= 1, "seed {seed:#x}");
    }
}

#[test]
fn failed_restore_into_running_process_leaves_live_state_unchanged() {
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (mut running, old, new, _) = build(seed, cap, n_old, n_new);
        let before = encode_context(&running).unwrap();
        let mut bad: Vec<Vec<u8>> = vec![
            Vec::new(),
            new[..new.len() - 1].to_vec(),
            old[..old.len() / 2].to_vec(),
        ];
        let mut flipped = new.clone();
        flipped[new.len() / 2] ^= 0x04;
        bad.push(flipped);
        for img in &bad {
            assert!(restore_into_sealed(&mut running, img).is_err());
            assert_eq!(encode_context(&running).unwrap(), before, "seed {seed:#x}");
        }
        assert_counters(&running, cap, n_new, "running");
    }
}

#[test]
fn restore_does_not_replay_admissions_or_side_effects() {
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (_, old, new, fx) = build(seed, cap, n_old, n_new);
        let mut ctx = fresh(seed, cap);
        for _ in 0..3 {
            restore_into_sealed(&mut ctx, &new).unwrap();
            assert_counters(&ctx, cap, n_new, "repeat new");
            restore_into_sealed(&mut ctx, &old).unwrap();
            assert_counters(&ctx, cap, n_old, "repeat old");
        }
        assert_eq!(fx.admits, n_new, "restore appended side effects");
        // Resuming after restoring old re-derives the same new checkpoint
        // only when the caller re-admits explicitly.
        let mut fx2 = Effects::default();
        admit_range(&mut ctx, &mut fx2, seed, n_old, n_new);
        assert_eq!(fx2.admits, n_new - n_old);
        assert_eq!(seal_context(&ctx).unwrap(), new, "seed {seed:#x}");
    }
}

/// Test-local model of external trusted monotonic state. NOT a product
/// feature: `sml-store` has no such anchor.
struct TrustedAnchor {
    highest_admitted: u64,
}

impl TrustedAnchor {
    fn admits(&self, ctx: &WorkingContext) -> bool {
        ctx.snapshot().stats.admitted >= self.highest_admitted
    }
}

#[test]
fn legitimate_rollback_is_not_detected_without_trusted_anchor() {
    for &(seed, cap, n_old, n_new) in CORPUS {
        let (_, old, new, _) = build(seed, cap, n_old, n_new);
        // `new` was durably committed...
        let mut disk = Disk::default();
        atomic_checkpoint(&mut disk, &old, Crash::AfterRename);
        atomic_checkpoint(&mut disk, &new, Crash::AfterRename);
        assert_eq!(disk.live.as_deref(), Some(&new[..]));
        // ...then an intact older snapshot is put back (stale replica,
        // restored backup, or adversary). Integrity check passes.
        disk.live = Some(old.clone());
        let (ctx, ok) = restart(&disk, seed, cap);
        assert!(
            ok,
            "seed {seed:#x}: intact old snapshot must pass integrity"
        );
        assert_counters(&ctx, cap, n_old, "rolled back");
        // Rollback detection is NOT implemented: nothing in the restore path
        // refused it. Only an external monotonic anchor would.
        let anchor = TrustedAnchor {
            highest_admitted: n_new,
        };
        assert!(
            !anchor.admits(&ctx),
            "seed {seed:#x}: anchor model must flag rollback"
        );
        let (fwd, _) = restart(
            &Disk {
                live: Some(new.clone()),
                tmp: None,
            },
            seed,
            cap,
        );
        assert!(anchor.admits(&fwd));
    }
}
