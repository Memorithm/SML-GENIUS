//! CTX2-INFER step 2 tests (prereg `docs/research/CTX2_INFER_PREREG.md`, section 11
//! test plan). Model-free: no weights, no runtime, no measurement (`NOT_MEASURED`).
//! The byte-level tokenizer below is a test fixture only, not the model tokenizer.

use std::collections::BTreeSet;

use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::ctx2_infer::audit::{
    a1_history_ids, audit_budget, check_server_prompt_count, AuditError, TokenizedCall,
};
use sml_store::ctx2_infer::compute::{ComputeEnvelope, N_CTX, N_PREDICT};
use sml_store::ctx2_infer::generator::{
    generate_seed, generate_session, session_file_sha256, Session, Split, TurnKind, BUCKETS,
    DEV_ATTRIBUTES, DEV_ENTITIES, QUERIES, TEST_ATTRIBUTES, TEST_ENTITIES, TURNS,
};
use sml_store::ctx2_infer::plan::{
    build_frozen_plan, build_grid, build_smoke_plan, context2_history, full_history, leaks_gold,
    sliding_window_history, stream_id, Arm, CallKey, CallLedger, InMemoryRawStore, Phase, Plan,
    K_VALUES,
};
use sml_store::ctx2_infer::restart::{
    checkpoint, context2_restart_history, restore_checkpoint, tamper_position, RestartControl,
    RESTART_TURNS,
};
use sml_store::ctx2_infer::score::{
    classify_h2, clustered_bootstrap_diff, exact_match, holm, mcnemar_one_sided, wilson95,
    H2Outcome, McNemarTable, BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED,
};
use sml_store::ctx2_infer::{
    derive_test_seed, dev_seeds, frozen_prereg, hex, Ctx2InferError, PreregMergeState,
    ANCHOR_STATUS, LOOP_STATUS, PREREG_CONTENT_COMMIT, PREREG_PR_HEAD, PREREG_SHA256_HEX, STATUS,
    TEST_SEED_DOMAIN,
};
use sml_store::working_context::{WorkingContext, WorkingPolicy};
use sml_store::working_context_anchor::AnchorState;

/// Exact bytes of the prereg in this checkout.
const PREREG_BYTES: &[u8] = include_bytes!("../../../docs/research/CTX2_INFER_PREREG.md");
/// Clearly synthetic merge SHA, used only to exercise seed derivation and grid size.
const SYNTHETIC_MERGE_SHA: &str = "0000000000000000000000000000000000c72fee";

/// Regression pins of this implementation (computed by this code, not measurements).
const DEV_SEEDS_PIN: [u64; 2] = [0x3fc3_d3ad_6bd3_e87a, 0x13f3_d0a2_cfec_e573];
const DEV0_SESSION_FILE_SHA256: &str =
    "f16eb9847eddf8ebf5cc936c54bb9e3b02a1a311bd0cd13ea84739b29eef5ba4";
const SMOKE_PLAN_DIGEST: &str = "864054b5f9de9295eb774b2c6e62a11a88f14feeef84a3dc3d3814898066a8a7";

fn dev0() -> Vec<Session> {
    generate_seed(dev_seeds()[0], Split::Dev).unwrap()
}

// ---------------------------------------------------------------- freeze

#[test]
fn statuses_are_not_measured_not_qualified_not_connected() {
    assert_eq!(STATUS, "NOT_MEASURED");
    assert_eq!(ANCHOR_STATUS, "NOT_QUALIFIED");
    assert_eq!(LOOP_STATUS, "NOT_CONNECTED");
}

#[test]
fn prereg_freeze_matches_repository_bytes() {
    let freeze = frozen_prereg();
    assert_eq!(freeze.path, "docs/research/CTX2_INFER_PREREG.md");
    assert_eq!(freeze.pr, 909);
    assert_eq!(freeze.content_commit, PREREG_CONTENT_COMMIT);
    assert_eq!(freeze.pr_head, PREREG_PR_HEAD);
    assert_eq!(hex(&sha256(PREREG_BYTES)), PREREG_SHA256_HEX);
    freeze.verify_bytes(PREREG_BYTES).unwrap();
    // The frozen text still states the parameters this module implements.
    let text = std::str::from_utf8(PREREG_BYTES).unwrap();
    for needle in [
        "PREREGISTERED, NOT EXECUTED, NOT_MEASURED",
        "`T = 48` turns",
        "`Q = 6` queries",
        "`d = T − j + 1`",
        "`k ∈ {4, 8, 16}`",
        "**27 648**",
        "**26 880**",
        "**72 hours**",
        "`0xC72_1AFE`",
        "CTX2-INFER-test-v1",
    ] {
        assert!(
            text.contains(needle),
            "prereg no longer contains {needle:?}"
        );
    }
}

#[test]
fn prereg_tamper_fails_closed() {
    let freeze = frozen_prereg();
    for pos in [0, PREREG_BYTES.len() / 2, PREREG_BYTES.len() - 1] {
        let mut bytes = PREREG_BYTES.to_vec();
        bytes[pos] ^= 0x20;
        assert!(matches!(
            freeze.verify_bytes(&bytes),
            Err(Ctx2InferError::PreregHashMismatch { .. })
        ));
    }
    let mut longer = PREREG_BYTES.to_vec();
    longer.push(b'\n');
    assert!(freeze.verify_bytes(&longer).is_err());
    assert!(freeze
        .verify_bytes(&PREREG_BYTES[..PREREG_BYTES.len() - 1])
        .is_err());
}

#[test]
fn test_seeds_fail_closed_until_merge_is_attested() {
    let freeze = frozen_prereg();
    assert_eq!(freeze.merge, PreregMergeState::Unmerged);
    assert_eq!(freeze.test_seeds(), Err(Ctx2InferError::PreregNotMerged));
    assert!(matches!(
        build_frozen_plan(&freeze),
        Err(Ctx2InferError::PreregNotMerged)
    ));
    for bad in [
        "",
        "EFF41FAB52196809139BD8B7646F0712A1DE6450",
        "eff41fab52196809139bd8b7646f0712a1de645",
        "eff41fab52196809139bd8b7646f0712a1de64500",
        "gff41fab52196809139bd8b7646f0712a1de6450",
    ] {
        assert_eq!(
            frozen_prereg().attest_merge(bad),
            Err(Ctx2InferError::InvalidCommitSha)
        );
        assert_eq!(
            derive_test_seed(bad, 0),
            Err(Ctx2InferError::InvalidCommitSha)
        );
    }
}

#[test]
fn test_seed_derivation_matches_independent_recomputation() {
    let freeze = frozen_prereg().attest_merge(SYNTHETIC_MERGE_SHA).unwrap();
    let seeds = freeze.test_seeds().unwrap();
    for (i, seed) in seeds.iter().enumerate() {
        let mut input = b"CTX2-INFER-test-v1".to_vec();
        assert_eq!(input, TEST_SEED_DOMAIN);
        input.extend_from_slice(SYNTHETIC_MERGE_SHA.as_bytes());
        input.push(i as u8);
        let d = sha256(&input);
        assert_eq!(*seed, u64::from_le_bytes(d[..8].try_into().unwrap()));
    }
    assert_eq!(seeds.iter().collect::<BTreeSet<_>>().len(), 5);
    // Disjoint from dev seeds.
    for d in dev_seeds() {
        assert!(!seeds.contains(&d));
    }
    // A different merge SHA gives different seeds.
    let other = frozen_prereg()
        .attest_merge("0000000000000000000000000000000000c72fef")
        .unwrap()
        .test_seeds()
        .unwrap();
    assert!(seeds.iter().zip(&other).all(|(a, b)| a != b));
}

#[test]
fn dev_seeds_are_pinned() {
    let seeds = dev_seeds();
    for (i, s) in seeds.iter().enumerate() {
        let mut input = b"CTX2-INFER-dev-v1".to_vec();
        input.push(i as u8);
        let d = sha256(&input);
        assert_eq!(*s, u64::from_le_bytes(d[..8].try_into().unwrap()));
    }
    assert_eq!(seeds, DEV_SEEDS_PIN);
}

// ---------------------------------------------------------------- generator

#[test]
fn generator_is_deterministic_and_pinned() {
    let a = dev0();
    let b = dev0();
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert_eq!(hex(&session_file_sha256(&a)), DEV0_SESSION_FILE_SHA256);
    let other = generate_seed(dev_seeds()[1], Split::Dev).unwrap();
    assert_ne!(session_file_sha256(&a), session_file_sha256(&other));
}

fn check_session(s: &Session) {
    assert_eq!(s.turns.len(), TURNS as usize);
    assert_eq!(s.queries.len(), QUERIES);
    let (entities, attributes) = match s.split {
        Split::Dev => (&DEV_ENTITIES[..], &DEV_ATTRIBUTES[..]),
        Split::Test => (&TEST_ENTITIES[..], &TEST_ATTRIBUTES[..]),
    };
    // Values: 4 ASCII digits, unique within the session.
    let values: BTreeSet<&str> = s.turns.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(values.len(), TURNS as usize);
    for (i, t) in s.turns.iter().enumerate() {
        assert_eq!(t.number, i as u32 + 1);
        assert!(t.value.len() == 4 && t.value.bytes().all(|c| c.is_ascii_digit()));
        assert!(entities.contains(&t.entity) && attributes.contains(&t.attribute));
        assert_eq!(
            t.bytes,
            format!(
                "Note {}: The {} of {} is {}.\n",
                t.number, t.attribute, t.entity, t.value
            )
            .into_bytes()
        );
    }
    // Buckets: each covered, one drawn twice; distinct answer turns.
    let mut per_bucket = [0_u32; 5];
    let mut answer_turns = BTreeSet::new();
    let queried: BTreeSet<(&str, &str)> =
        s.queries.iter().map(|q| (q.entity, q.attribute)).collect();
    assert_eq!(queried.len(), QUERIES);
    for q in &s.queries {
        per_bucket[q.bucket] += 1;
        assert!(answer_turns.insert(q.answer_turn));
        assert_eq!(q.distance, TURNS - q.answer_turn + 1);
        let (lo, hi) = BUCKETS[q.bucket];
        assert!((lo..=hi).contains(&q.distance));
        // Gold = latest assignment of the key; it appears in exactly one turn.
        let assigns: Vec<_> = s
            .turns
            .iter()
            .filter(|t| t.entity == q.entity && t.attribute == q.attribute)
            .collect();
        let last = assigns.last().unwrap();
        assert_eq!(last.number, q.answer_turn);
        assert_eq!(last.value, q.gold);
        assert!(assigns.len() <= 2);
        assert!(matches!(last.kind, TurnKind::Set | TurnKind::Update));
        if assigns.len() == 2 {
            assert_eq!(assigns[0].kind, TurnKind::Set);
            assert_eq!(last.kind, TurnKind::Update);
        } else {
            assert_eq!(last.kind, TurnKind::Set);
        }
        let hits = s
            .turns
            .iter()
            .filter(|t| leaks_gold(&t.bytes, &q.gold))
            .count();
        assert_eq!(hits, 1);
        assert_eq!(
            q.text,
            format!(
                "What is the {} of {}? Answer with the number only.",
                q.attribute, q.entity
            )
        );
    }
    assert!(per_bucket.iter().all(|&n| n >= 1));
    assert_eq!(per_bucket.iter().filter(|&&n| n == 2).count(), 1);
    for t in &s.turns {
        if t.kind == TurnKind::Distractor {
            assert!(!queried.contains(&(t.entity, t.attribute)));
        }
    }
}

#[test]
fn generator_invariants_hold_for_dev_and_test_splits() {
    for s in dev0() {
        check_session(&s);
    }
    let test_seed = derive_test_seed(SYNTHETIC_MERGE_SHA, 0).unwrap();
    for s in generate_seed(test_seed, Split::Test).unwrap() {
        check_session(&s);
    }
}

#[test]
fn dev_and_test_vocabularies_are_disjoint() {
    let dev: BTreeSet<&str> = DEV_ENTITIES
        .iter()
        .chain(&DEV_ATTRIBUTES)
        .copied()
        .collect();
    for w in TEST_ENTITIES.iter().chain(&TEST_ATTRIBUTES) {
        assert!(!dev.contains(w), "{w} in both splits");
    }
}

// ---------------------------------------------------------------- arms

#[test]
fn distance_convention_matches_window_membership_including_boundaries() {
    let mut boundary_retained = 0;
    let mut boundary_evicted = 0;
    for s in dev0() {
        for k in K_VALUES {
            let (ids, _) = sliding_window_history(&s, k);
            for q in &s.queries {
                let inside = ids.contains(&u64::from(q.answer_turn));
                assert_eq!(q.retained(k), inside, "d={} k={k}", q.distance);
                assert_eq!(q.distance <= k, inside);
                if q.distance == k {
                    assert!(inside);
                    boundary_retained += 1;
                }
                if q.distance == k + 1 {
                    assert!(!inside);
                    boundary_evicted += 1;
                }
            }
        }
    }
    assert!(boundary_retained > 0 && boundary_evicted > 0);
    // Worked example from the prereg: T = 48, k = 4 keeps turns 45..48.
    let s = generate_session(dev_seeds()[0], Split::Dev, 0).unwrap();
    assert_eq!(sliding_window_history(&s, 4).0, vec![45, 46, 47, 48]);
}

#[test]
fn h1_context2_prompt_bytes_equal_sliding_window_for_all_sessions_and_k() {
    for seed in dev_seeds() {
        for s in generate_seed(seed, Split::Dev).unwrap() {
            let store = InMemoryRawStore::publish_session(&s).unwrap();
            for k in K_VALUES {
                assert_eq!(
                    context2_history(&s, k, &store).unwrap(),
                    sliding_window_history(&s, k)
                );
            }
            // Capacity >= T keeps everything (equals R).
            assert_eq!(context2_history(&s, 64, &store).unwrap(), full_history(&s));
        }
    }
}

#[test]
fn h3a_gold_absent_from_evicted_histories_and_present_when_retained() {
    let mut evicted = 0;
    let mut retained = 0;
    for s in dev0() {
        let store = InMemoryRawStore::publish_session(&s).unwrap();
        for k in K_VALUES {
            let a2 = sliding_window_history(&s, k).1;
            let a3 = context2_history(&s, k, &store).unwrap().1;
            let a3r = context2_restart_history(&s, k, &store, &RestartControl::Clean)
                .unwrap()
                .history
                .1;
            for q in &s.queries {
                for h in [&a2, &a3, &a3r] {
                    assert_eq!(leaks_gold(h, &q.gold), q.distance <= k);
                }
                if q.distance > k {
                    evicted += 1;
                } else {
                    retained += 1;
                }
            }
        }
    }
    assert!(evicted > 0 && retained > 0);
}

// ---------------------------------------------------------------- restart / tamper

#[test]
fn restart_clean_path_reproduces_a3_with_three_anchored_checkpoints() {
    for s in dev0().iter().take(16) {
        let store = InMemoryRawStore::publish_session(s).unwrap();
        for k in K_VALUES {
            let out = context2_restart_history(s, k, &store, &RestartControl::Clean).unwrap();
            assert_eq!(out.history, context2_history(s, k, &store).unwrap());
            let turns: Vec<u32> = out.checkpoints.iter().map(|c| c.turn).collect();
            assert_eq!(turns, RESTART_TURNS.to_vec());
            let gens: Vec<u64> = out.checkpoints.iter().map(|c| c.generation).collect();
            assert_eq!(gens, vec![12, 24, 36]);
        }
    }
}

fn refused_at(r: Result<impl std::fmt::Debug, Ctx2InferError>, turn: u32) -> String {
    match r {
        Err(Ctx2InferError::RestartRefused { turn: t, reason }) => {
            assert_eq!(t, turn);
            reason
        }
        other => panic!("expected refusal at turn {turn}, got {other:?}"),
    }
}

#[test]
fn restart_negative_controls_fail_closed_with_no_prompt() {
    let sessions = dev0();
    let s = &sessions[3];
    let foreign_session = &sessions[4];
    let store = InMemoryRawStore::publish_session(s).unwrap();
    let k = 8;
    let clean = context2_restart_history(s, k, &store, &RestartControl::Clean).unwrap();
    for (i, &turn) in RESTART_TURNS.iter().enumerate() {
        let len = clean.checkpoints[i].envelope_len;
        let position = tamper_position(s.seed, len);
        assert!(position < len);
        refused_at(
            context2_restart_history(s, k, &store, &RestartControl::FlipByte { turn, position }),
            turn,
        );
        for cut in [0, 1, len / 2, len - 1] {
            refused_at(
                context2_restart_history(
                    s,
                    k,
                    &store,
                    &RestartControl::Truncate { turn, len: cut },
                ),
                turn,
            );
        }
        let r = refused_at(
            context2_restart_history(s, k, &store, &RestartControl::AnchorAbsent { turn }),
            turn,
        );
        assert!(r.contains("AnchorAbsent"), "{r}");
        let r = refused_at(
            context2_restart_history(s, k, &store, &RestartControl::AnchorUnverifiable { turn }),
            turn,
        );
        assert!(r.contains("AnchorUnverifiable"), "{r}");
        // Envelope sealed by another session's stream at the same turn.
        let fstream = stream_id(foreign_session).unwrap();
        let mut fctx =
            WorkingContext::new(fstream, WorkingPolicy::CapacityBounded { capacity: 8 }).unwrap();
        for t in &foreign_session.turns[..turn as usize] {
            sml_store::ctx2_infer::plan::admit_turn(&mut fctx, fstream, t).unwrap();
        }
        let (envelope, _) = checkpoint(&fctx, None, turn).unwrap();
        let r = refused_at(
            context2_restart_history(
                s,
                k,
                &store,
                &RestartControl::ForeignEnvelope { turn, envelope },
            ),
            turn,
        );
        assert!(r.contains("StreamMismatch"), "{r}");
    }
    // Rollback: presenting the turn-12 envelope at turn 24 / the turn-24 one at 36.
    for turn in [24, 36] {
        let r = refused_at(
            context2_restart_history(s, k, &store, &RestartControl::Rollback { turn }),
            turn,
        );
        assert!(r.contains("Rollback"), "{r}");
    }
    // No earlier checkpoint exists at the first restart: still refused.
    refused_at(
        context2_restart_history(s, k, &store, &RestartControl::Rollback { turn: 12 }),
        12,
    );
}

#[test]
fn every_single_byte_envelope_flip_is_refused_and_live_context_is_unchanged() {
    let s = &dev0()[7];
    let stream = stream_id(s).unwrap();
    let policy = WorkingPolicy::CapacityBounded { capacity: 4 };
    let mut sealed_ctx = WorkingContext::new(stream, policy).unwrap();
    for t in &s.turns[..12] {
        sml_store::ctx2_infer::plan::admit_turn(&mut sealed_ctx, stream, t).unwrap();
    }
    let (envelope, anchor) = checkpoint(&sealed_ctx, None, 12).unwrap();
    let state = AnchorState::Verified(anchor);
    let mut live = WorkingContext::new(stream, policy).unwrap();
    for t in &s.turns[..5] {
        sml_store::ctx2_infer::plan::admit_turn(&mut live, stream, t).unwrap();
    }
    let before = live.snapshot();
    for pos in 0..envelope.len() {
        let mut bad = envelope.clone();
        bad[pos] ^= 0x01;
        assert!(
            restore_checkpoint(&mut live, &bad, &state, 12).is_err(),
            "flip at {pos} accepted"
        );
        assert_eq!(
            live.snapshot(),
            before,
            "flip at {pos} mutated live context"
        );
    }
    // Positive control: the untouched envelope is admitted.
    restore_checkpoint(&mut live, &envelope, &state, 12).unwrap();
    assert_eq!(live.snapshot(), sealed_ctx.snapshot());
}

#[test]
fn raw_store_tamper_fails_prompt_assembly_closed() {
    let s = &dev0()[9];
    let sid = stream_id(s).unwrap().get();
    let store = InMemoryRawStore::publish_session(s).unwrap();
    let k = 4;
    // Retained segments are 45..48. Corrupting any byte of a retained artifact fails.
    for (seg, pos) in [(45, 0), (46, 100), (47, 150), (48, 160)] {
        let mut bad = store.clone();
        bad.corrupt_for_negative_control(sid, seg, pos);
        assert!(matches!(
            context2_history(s, k, &bad),
            Err(Ctx2InferError::RawStore { segment_id, .. }) if segment_id == seg
        ));
        assert!(context2_restart_history(s, k, &bad, &RestartControl::Clean).is_err());
    }
    let mut missing = store.clone();
    missing.remove_for_negative_control(sid, 47);
    assert_eq!(
        context2_history(s, k, &missing),
        Err(Ctx2InferError::MissingSegment {
            stream_id: sid,
            segment_id: 47
        })
    );
    let mut swapped = store.clone();
    swapped.swap_for_negative_control(sid, 46, 47);
    assert!(context2_history(s, k, &swapped).is_err());
    // A retained segment swapped with an evicted one is also caught.
    let mut swapped = store.clone();
    swapped.swap_for_negative_control(sid, 48, 1);
    assert!(context2_history(s, k, &swapped).is_err());
    // Untouched store still assembles.
    assert_eq!(
        context2_history(s, k, &store).unwrap(),
        sliding_window_history(s, k)
    );
}

// ---------------------------------------------------------------- token audit

/// Test-only byte-level tokenizer (id = byte + 3). NOT the model tokenizer.
fn tok(bytes: &[u8]) -> Vec<u32> {
    bytes.iter().map(|&b| u32::from(b) + 3).collect()
}

/// Test-only tokenizer that merges pairs of bytes, so token cuts need not align with turns.
fn tok_pairs(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(2)
        .map(|c| c.iter().fold(7_u32, |a, &b| a * 257 + u32::from(b)))
        .collect()
}

const T_PRE: [u32; 3] = [1, 900, 901];
const T_POST_TAIL: [u32; 4] = [902, 2, 903, 904];

fn tokenize_plan(plan: &Plan, tokenizer: fn(&[u8]) -> Vec<u32>) -> Vec<TokenizedCall> {
    // B(s, k) from A3 under the same tokenizer.
    let mut budget = std::collections::BTreeMap::new();
    for r in &plan.records {
        if r.key.arm == Arm::A3 {
            budget.insert(
                (r.key.phase, r.key.seed, r.key.session, r.key.k),
                tokenizer(&r.history).len(),
            );
        }
    }
    plan.records
        .iter()
        .map(|r| {
            let full = tokenizer(&r.history);
            let h = if r.key.arm == Arm::A1 {
                let b = budget[&(r.key.phase, r.key.seed, r.key.session, r.key.k)];
                a1_history_ids(&full, b).unwrap()
            } else {
                full
            };
            let mut t_post = tokenizer(r.query_text.as_bytes());
            t_post.extend_from_slice(&T_POST_TAIL);
            TokenizedCall {
                key: r.key,
                source_sha256: r.history_sha256,
                t_pre: T_PRE.to_vec(),
                h,
                t_post,
            }
        })
        .collect()
}

#[test]
fn budget_audit_passes_on_equal_budget_arms_and_reports_totals() {
    let plan = build_smoke_plan(dev_seeds()[0]).unwrap();
    let calls = tokenize_plan(&plan, tok);
    let report = audit_budget(&plan, &calls, N_CTX, N_PREDICT).unwrap();
    assert_eq!(report.calls, 84);
    assert_eq!(report.a1_total, 12);
    // Byte-level fixture: the A1 cut lands on a turn boundary, so H equals A2's H.
    assert_eq!(report.a1_equals_a2, 12);
    let seed = dev_seeds()[0];
    let ph = Phase::Smoke { run: 0 };
    let p = |arm| report.totals[&(ph, arm, Some(4), seed)];
    assert_eq!(p(Arm::A1).prompt_tokens, p(Arm::A2).prompt_tokens);
    assert_eq!(p(Arm::A2).prompt_tokens, p(Arm::A3).prompt_tokens);
    assert_eq!(p(Arm::A3).prompt_tokens, p(Arm::A3r).prompt_tokens);
    assert_eq!(report.totals[&(ph, Arm::A0, None, seed)].history_tokens, 0);
    assert!(report.totals[&(ph, Arm::R, None, seed)].history_tokens > p(Arm::A3).history_tokens);
    // Prompt-level H1 / H2p on exact token arrays.
    for c in &calls {
        if matches!(c.key.arm, Arm::A3 | Arm::A3r) {
            let a2 = calls
                .iter()
                .find(|x| {
                    x.key
                        == CallKey {
                            arm: Arm::A2,
                            ..c.key
                        }
                })
                .unwrap();
            assert_eq!(c.prompt_sha256(), a2.prompt_sha256());
        }
        check_server_prompt_count(c, c.prompt_len()).unwrap();
        assert_eq!(c.prompt_len(), c.t_pre.len() + c.h.len() + c.t_post.len());
    }
    // Pair-merging fixture: still equal budgets; A1 vs A2 identity is only reported.
    let calls = tokenize_plan(&plan, tok_pairs);
    let report = audit_budget(&plan, &calls, N_CTX, N_PREDICT).unwrap();
    assert!(report.a1_equals_a2 <= report.a1_total);
}

#[test]
fn budget_audit_on_one_seed_test_grid() {
    let seed = derive_test_seed(SYNTHETIC_MERGE_SHA, 0).unwrap();
    let plan = build_grid(&[seed], dev_seeds()[0]).unwrap();
    assert_eq!(plan.records.len(), 768 + 14 * 384);
    let calls = tokenize_plan(&plan, tok);
    let report = audit_budget(&plan, &calls, N_CTX, N_PREDICT).unwrap();
    assert_eq!(report.calls, plan.records.len() as u64);
    assert!(report.max_prompt_len as u32 + N_PREDICT <= N_CTX);
}

#[test]
fn budget_audit_negative_controls_fail_closed() {
    let plan = build_smoke_plan(dev_seeds()[0]).unwrap();
    let good = tokenize_plan(&plan, tok);
    let idx = |arm| good.iter().position(|c| c.key.arm == arm).unwrap();
    let run = |calls: &[TokenizedCall]| audit_budget(&plan, calls, N_CTX, N_PREDICT);

    for arm in [Arm::A1, Arm::A2, Arm::A3r] {
        let mut c = good.clone();
        c[idx(arm)].h.pop();
        assert!(
            matches!(run(&c), Err(AuditError::BudgetMismatch { .. })),
            "{arm:?}"
        );
    }
    let mut c = good.clone();
    c[idx(Arm::A3)].h.pop();
    assert!(matches!(run(&c), Err(AuditError::InconsistentA3Budget(_))));

    let mut c = good.clone();
    c[idx(Arm::A2)].t_post.push(5);
    assert!(matches!(run(&c), Err(AuditError::TemplateMismatch(_))));
    let mut c = good.clone();
    c[idx(Arm::A3)].t_pre[0] = 0;
    assert!(matches!(run(&c), Err(AuditError::TemplateMismatch(_))));

    let mut c = good.clone();
    c[idx(Arm::A0)].h.push(42);
    assert!(matches!(run(&c), Err(AuditError::NonEmptyA0(_))));

    let mut c = good.clone();
    let removed = c.remove(idx(Arm::A3));
    assert_eq!(run(&c), Err(AuditError::MissingCall(removed.key)));

    let mut c = good.clone();
    c.push(good[0].clone());
    assert_eq!(run(&c), Err(AuditError::DuplicateCall(good[0].key)));

    let mut c = good.clone();
    let mut extra = good[0].clone();
    extra.key.seed ^= 1;
    c.push(extra.clone());
    assert_eq!(run(&c), Err(AuditError::UnplannedCall(extra.key)));

    let mut c = good.clone();
    c[idx(Arm::A3r)].source_sha256[0] ^= 1;
    assert!(matches!(run(&c), Err(AuditError::SourceMismatch(_))));

    let max = good.iter().map(TokenizedCall::prompt_len).max().unwrap() as u32;
    assert!(audit_budget(&plan, &good, max + N_PREDICT, N_PREDICT).is_ok());
    assert!(matches!(
        audit_budget(&plan, &good, max + N_PREDICT - 1, N_PREDICT),
        Err(AuditError::ContextOverflow { .. })
    ));

    let c = &good[idx(Arm::A3)];
    for reported in [c.prompt_len() - 1, c.prompt_len() + 1] {
        assert!(matches!(
            check_server_prompt_count(c, reported),
            Err(AuditError::ServerPromptCount { .. })
        ));
    }
    assert_eq!(
        a1_history_ids(&[1, 2, 3], 4),
        Err(AuditError::A1BudgetExceedsHistory {
            budget: 4,
            available: 3
        })
    );
    assert_eq!(a1_history_ids(&[1, 2, 3], 2).unwrap(), vec![2, 3]);
    assert!(a1_history_ids(&[1, 2, 3], 0).unwrap().is_empty());
}

// ---------------------------------------------------------------- plan / envelope / ledger

#[test]
fn compute_envelope_is_derived_and_matches_prereg() {
    let env = ComputeEnvelope::derive();
    assert_eq!(env, ComputeEnvelope::FROZEN);
    assert_eq!(env.naive_grid_calls - env.test_calls, 7_680);
    assert_eq!(env.ceiling_secs, 72 * 3600);
    assert_eq!(env.check_wall_clock(1, 3600), Ok(()));
    assert!(env.check_wall_clock(1, 3601).is_err());
    assert!(env.check_wall_clock(72, 72 * 3600 + 1).is_err());
    assert!(env.check_wall_clock(1000, 72 * 3600 + 1).is_err());
    assert_eq!(N_CTX, 4096);
    assert_eq!(N_PREDICT, 8);
}

#[test]
fn full_grid_call_count_equals_envelope_and_structural_invariants_hold() {
    let freeze = frozen_prereg().attest_merge(SYNTHETIC_MERGE_SHA).unwrap();
    let plan = build_frozen_plan(&freeze).unwrap();
    let env = ComputeEnvelope::FROZEN;
    assert_eq!(plan.records.len() as u64, env.admitted_host_calls);
    let count =
        |f: &dyn Fn(&CallKey) -> bool| plan.records.iter().filter(|r| f(&r.key)).count() as u64;
    assert_eq!(count(&|k| k.phase == Phase::Test), env.test_calls);
    assert_eq!(
        count(&|k| matches!(k.phase, Phase::Precheck { .. })),
        env.precheck_calls
    );
    assert_eq!(
        count(&|k| k.phase == Phase::Test && k.arm == Arm::A0),
        env.a0_calls
    );
    assert_eq!(
        count(&|k| k.phase == Phase::Test && k.arm == Arm::R),
        env.r_calls
    );
    assert_eq!(
        count(&|k| k.phase == Phase::Test && k.k.is_some()),
        env.budgeted_arm_calls
    );
    let keys: BTreeSet<CallKey> = plan.records.iter().map(|r| r.key).collect();
    assert_eq!(keys.len(), plan.records.len());
    let s = plan.summary();
    assert_eq!((s.h1_identical, s.h1_total), (5760, 5760));
    assert_eq!((s.h2p_identical, s.h2p_total), (5760, 5760));
    assert_eq!(s.h3a_leaks, 0);
    assert!(s.h3a_checked > 0);
    plan.verify_digest(&plan.digest).unwrap();
}

#[test]
fn smoke_plan_is_pinned_and_plan_tamper_fails_closed() {
    let plan = build_smoke_plan(dev_seeds()[0]).unwrap();
    assert_eq!(
        plan.records.len() as u64,
        ComputeEnvelope::FROZEN.smoke_calls
    );
    assert_eq!(hex(&plan.digest), SMOKE_PLAN_DIGEST);
    assert_eq!(
        build_smoke_plan(dev_seeds()[0]).unwrap().digest,
        plan.digest
    );
    let pinned = plan.digest;
    let mut bad = plan.clone();
    bad.records[5].gold = "0000".to_owned();
    assert_eq!(
        bad.verify_digest(&pinned),
        Err(Ctx2InferError::PlanDigestMismatch)
    );
    let mut bad = plan.clone();
    bad.records.swap(0, 1);
    assert_eq!(
        bad.verify_digest(&pinned),
        Err(Ctx2InferError::PlanDigestMismatch)
    );
    let mut bad = plan.clone();
    bad.records.pop();
    assert_eq!(
        bad.verify_digest(&pinned),
        Err(Ctx2InferError::PlanDigestMismatch)
    );
    let mut wrong = pinned;
    wrong[0] ^= 1;
    assert_eq!(
        plan.verify_digest(&wrong),
        Err(Ctx2InferError::PlanDigestMismatch)
    );
}

#[test]
fn call_ledger_enforces_plan_retry_and_count() {
    let plan = build_smoke_plan(dev_seeds()[0]).unwrap();
    let mut ledger = CallLedger::new(&plan);
    let k0 = plan.records[0].key;
    let k1 = plan.records[1].key;
    let mut unplanned = k0;
    unplanned.session = 999;
    assert!(ledger.issue(&unplanned).is_err());
    assert_eq!(ledger.issue(&k0).unwrap(), 1);
    assert!(ledger.issue(&k0).is_err(), "in flight twice");
    ledger.transport_failure(&k0).unwrap();
    assert_eq!(ledger.issue(&k0).unwrap(), 2);
    assert!(
        ledger.transport_failure(&k0).is_err(),
        "second failure is unrecoverable"
    );
    assert_eq!(ledger.issue(&k1).unwrap(), 1);
    ledger.complete(&k1).unwrap();
    assert!(ledger.issue(&k1).is_err(), "duplicate");
    assert!(ledger.complete(&k1).is_err());
    assert!(ledger.finish().is_err(), "incomplete");

    let mut ledger = CallLedger::new(&plan);
    for (i, r) in plan.records.iter().enumerate() {
        ledger.issue(&r.key).unwrap();
        if i == 3 {
            ledger.transport_failure(&r.key).unwrap();
            ledger.issue(&r.key).unwrap();
        }
        ledger.complete(&r.key).unwrap();
    }
    assert_eq!(ledger.finish().unwrap(), (84, 1));
}

// ---------------------------------------------------------------- score

#[test]
fn exact_match_uses_first_maximal_digit_run() {
    assert!(exact_match("1234", "1234"));
    assert!(exact_match(" The answer is 1234.", "1234"));
    assert!(exact_match("0042\n", "0042"));
    assert!(!exact_match("12345", "1234"));
    assert!(!exact_match("123 4", "1234"));
    assert!(!exact_match("4321 1234", "1234"));
    assert!(!exact_match("no digits", "1234"));
    assert!(!exact_match("", "1234"));
    assert!(!exact_match("42", "0042"));
}

#[test]
fn wilson_interval_matches_hand_values() {
    assert_eq!(wilson95(0, 0), None);
    assert_eq!(wilson95(3, 2), None);
    let (lo, hi) = wilson95(0, 10).unwrap();
    assert!(lo.abs() < 1e-12 && (hi - 0.277_532).abs() < 1e-5, "{hi}");
    let (lo, hi) = wilson95(5, 10).unwrap();
    assert!((lo - 0.236_593).abs() < 1e-5 && (hi - 0.763_407).abs() < 1e-5);
    let (lo, hi) = wilson95(10, 10).unwrap();
    assert!((lo - 0.722_467).abs() < 1e-5 && (hi - 1.0).abs() < 1e-12);
}

#[test]
fn mcnemar_one_sided_matches_hand_tables() {
    assert_eq!(mcnemar_one_sided(0, 0), 1.0);
    assert!((mcnemar_one_sided(5, 0) - 1.0 / 32.0).abs() < 1e-12);
    assert!((mcnemar_one_sided(0, 5) - 1.0).abs() < 1e-12);
    assert!((mcnemar_one_sided(3, 1) - 5.0 / 16.0).abs() < 1e-12);
    assert!((mcnemar_one_sided(1, 3) - 15.0 / 16.0).abs() < 1e-12);
    assert!((mcnemar_one_sided(10, 10) - 616_666.0 / 1_048_576.0).abs() < 1e-12);
    // Large n stays finite and monotone in b.
    let a = mcnemar_one_sided(1000, 920);
    let b = mcnemar_one_sided(1001, 919);
    assert!(a.is_finite() && b < a && a < 0.05);
    let t = McNemarTable::from_pairs(&[
        (true, true),
        (true, false),
        (true, false),
        (false, true),
        (false, false),
    ]);
    assert_eq!((t.n11, t.b, t.c, t.n00), (1, 2, 1, 1));
    assert!((t.p_one_sided() - 0.5).abs() < 1e-12);
}

#[test]
fn holm_matches_hand_values() {
    let adj = holm(&[0.01, 0.04, 0.03, 0.005]);
    let want = [0.03, 0.06, 0.06, 0.02];
    for (a, w) in adj.iter().zip(want) {
        assert!((a - w).abs() < 1e-12, "{adj:?}");
    }
    assert_eq!(holm(&[0.9, 0.8]), vec![1.0, 1.0]);
    assert!(holm(&[]).is_empty());
}

#[test]
fn clustered_bootstrap_is_deterministic_and_bounded() {
    let all_tie = vec![vec![(true, true), (false, false)]; 8];
    assert_eq!(
        clustered_bootstrap_diff(&all_tie, 200, BOOTSTRAP_SEED),
        Some((0.0, 0.0))
    );
    let all_win = vec![vec![(true, false); 3]; 5];
    assert_eq!(
        clustered_bootstrap_diff(&all_win, 200, BOOTSTRAP_SEED),
        Some((1.0, 1.0))
    );
    let mixed: Vec<Vec<(bool, bool)>> = (0..20)
        .map(|i| vec![(i % 3 == 0, i % 2 == 0), (i % 5 == 0, false)])
        .collect();
    let a = clustered_bootstrap_diff(&mixed, BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED).unwrap();
    let b = clustered_bootstrap_diff(&mixed, BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED).unwrap();
    assert_eq!(a, b);
    assert!(a.0 <= a.1 && a.0 >= -1.0 && a.1 <= 1.0);
    assert_eq!(clustered_bootstrap_diff(&[], 10, BOOTSTRAP_SEED), None);
    assert_eq!(
        clustered_bootstrap_diff(&[vec![]], 10, BOOTSTRAP_SEED),
        None
    );
}

#[test]
fn strict_h2_classification_never_passes_without_precheck() {
    use H2Outcome::{Fail, Indeterminate, Pass};
    assert_eq!(classify_h2(true, false, false, false), Pass);
    assert_eq!(classify_h2(true, false, false, true), Fail);
    assert_eq!(classify_h2(false, false, false, false), Indeterminate);
    assert_eq!(classify_h2(false, false, false, true), Indeterminate);
    assert_eq!(classify_h2(false, true, false, false), Fail);
    assert_eq!(classify_h2(false, false, true, false), Fail);
    assert_eq!(classify_h2(true, true, true, true), Fail);
}
