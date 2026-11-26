//! Transactional cumulative knowledge store.
//!
//! Acquisitions may be computed in parallel, but publication into the shared
//! memory goes through [`KnowledgeStore::commit`], which provides:
//!
//! * **certificate gating** — a [`StagedEntry`] can only be built from a
//!   [`Certificate`] minted by the examiner;
//! * **optimistic concurrency** — a transaction names the version it was built
//!   on; a stale base is rejected with [`StoreError::Conflict`] and the caller
//!   rebases (no lost update);
//! * **mutual exclusion** across threads and processes via an exclusive
//!   `LOCK` file (owner record hard-linked into place); a lock left by a
//!   dead process on the same host is recovered, any other lock fails closed;
//! * **safe initialisation** — the first `HEAD` is written under the lock;
//! * **idempotence** — re-publishing an identical entry is a no-op;
//! * **contradiction detection** — the same knowledge key with different
//!   content aborts the whole transaction;
//! * **atomicity and rollback** — immutable content-addressed objects and
//!   manifests are written first, and the transaction becomes visible only
//!   through the final atomic rename of `HEAD`. Any failure before that rename
//!   leaves the previous version intact;
//! * **immutable, restorable history** — every manifest records its parent;
//!   [`KnowledgeStore::restore`] republishes an earlier version as a new one.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::hash::Digest;
use crate::lesson::LessonId;
use crate::mastery::Certificate;

const MANIFEST_MAGIC: &str = "SML_TUTOR_MANIFEST_V1";
const HEAD_MAGIC: &str = "SML_TUTOR_HEAD_V1";

type ParsedManifest = (u64, Option<Digest>, BTreeMap<Digest, EntryRecord>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryRecord {
    pub key: Digest,
    pub lesson: LessonId,
    pub certificate: Digest,
    pub object: Digest,
}

/// An entry ready for publication. Only constructible from a certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedEntry {
    record: EntryRecord,
    payload: Vec<u8>,
}

impl StagedEntry {
    pub fn new(certificate: &Certificate, key: Digest, payload: Vec<u8>) -> Self {
        Self {
            record: EntryRecord {
                key,
                lesson: certificate.lesson(),
                certificate: certificate.digest(),
                object: Digest::of(&payload),
            },
            payload,
        }
    }

    pub fn record(&self) -> &EntryRecord {
        &self.record
    }
}

#[derive(Clone, Debug, Default)]
pub struct Transaction {
    pub base_version: u64,
    pub entries: Vec<StagedEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitOutcome {
    pub version: u64,
    pub added: usize,
    pub idempotent: usize,
}

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Busy,
    Conflict {
        base: u64,
        current: u64,
    },
    Contradiction {
        key: Digest,
        existing: Digest,
        proposed: Digest,
    },
    Corrupt(String),
    UnknownVersion(u64),
    InjectedFailure(FailPoint),
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
            StoreError::Busy => f.write_str("store locked by another writer"),
            StoreError::Conflict { base, current } => {
                write!(f, "stale base version {base}, current is {current}")
            }
            StoreError::Contradiction {
                key,
                existing,
                proposed,
            } => write!(
                f,
                "contradiction on key {}: existing {} vs proposed {}",
                key.short(),
                existing.short(),
                proposed.short()
            ),
            StoreError::Corrupt(m) => write!(f, "corrupt store: {m}"),
            StoreError::UnknownVersion(v) => write!(f, "unknown version {v}"),
            StoreError::InjectedFailure(p) => write!(f, "injected failure at {p:?}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// Test-only crash simulation points inside a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailPoint {
    AfterObjects,
    AfterManifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub version: u64,
    pub manifest: Option<Digest>,
    pub entries: BTreeMap<Digest, EntryRecord>,
}

pub struct KnowledgeStore {
    root: PathBuf,
    fail_point: Option<FailPoint>,
}

struct LockGuard(PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dir.join(name))?;
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn host_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .or_else(|_| fs::read_to_string("/etc/hostname"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn lock_owner_record() -> String {
    format!("SML_TUTOR_LOCK_V1 {} {}\n", std::process::id(), host_id())
}

/// True only when the lock owner is on this host (same boot) and its pid is no
/// longer alive. Uses `/proc`, so off Linux this always answers false.
fn owner_is_dead(record: &str) -> bool {
    let mut parts = record.split_whitespace();
    if parts.next() != Some("SML_TUTOR_LOCK_V1") {
        return false;
    }
    let Some(pid) = parts.next().and_then(|p| p.parse::<u32>().ok()) else {
        return false;
    };
    let host = parts.next().unwrap_or("");
    let here = host_id();
    if here.is_empty() || host != here || !Path::new("/proc/self").exists() {
        return false;
    }
    !Path::new(&format!("/proc/{pid}")).exists()
}

impl KnowledgeStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("manifests"))?;
        let store = Self {
            root,
            fail_point: None,
        };
        if !store.root.join("HEAD").exists() {
            // Initialise under the writer lock and re-check, so a concurrent
            // opener can never overwrite a HEAD another writer already advanced.
            let _guard = store.lock_waiting(10_000)?;
            if !store.root.join("HEAD").exists() {
                write_atomic(
                    &store.root,
                    "HEAD",
                    format!("{HEAD_MAGIC}\n0 none\n").as_bytes(),
                )?;
            }
        }
        store.read_head()?;
        Ok(store)
    }

    #[doc(hidden)]
    pub fn inject_failure(&mut self, point: Option<FailPoint>) {
        self.fail_point = point;
    }

    fn read_head(&self) -> Result<(u64, Option<Digest>), StoreError> {
        let text = fs::read_to_string(self.root.join("HEAD"))?;
        let mut lines = text.lines();
        if lines.next() != Some(HEAD_MAGIC) {
            return Err(StoreError::Corrupt("bad HEAD magic".into()));
        }
        let line = lines
            .next()
            .ok_or_else(|| StoreError::Corrupt("truncated HEAD".into()))?;
        let (v, m) = line
            .split_once(' ')
            .ok_or_else(|| StoreError::Corrupt("bad HEAD line".into()))?;
        let version: u64 = v
            .parse()
            .map_err(|_| StoreError::Corrupt("bad HEAD version".into()))?;
        let manifest = if m == "none" {
            None
        } else {
            Some(Digest::from_hex(m).ok_or_else(|| StoreError::Corrupt("bad HEAD digest".into()))?)
        };
        if (version == 0) != manifest.is_none() {
            return Err(StoreError::Corrupt("HEAD version/manifest mismatch".into()));
        }
        Ok((version, manifest))
    }

    pub fn version(&self) -> Result<u64, StoreError> {
        Ok(self.read_head()?.0)
    }

    fn read_manifest(&self, d: &Digest) -> Result<ParsedManifest, StoreError> {
        let bytes = fs::read(self.root.join("manifests").join(d.to_hex()))?;
        if Digest::of(&bytes) != *d {
            return Err(StoreError::Corrupt(format!(
                "manifest {} hash mismatch",
                d.short()
            )));
        }
        let text =
            String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("manifest utf8".into()))?;
        let mut lines = text.lines();
        let bad = || StoreError::Corrupt(format!("manifest {} malformed", d.short()));
        if lines.next() != Some(MANIFEST_MAGIC) {
            return Err(bad());
        }
        let version: u64 = lines
            .next()
            .and_then(|l| l.strip_prefix("version "))
            .and_then(|v| v.parse().ok())
            .ok_or_else(bad)?;
        let parent = match lines.next().and_then(|l| l.strip_prefix("parent ")) {
            Some("none") => None,
            Some(h) => Some(Digest::from_hex(h).ok_or_else(bad)?),
            None => return Err(bad()),
        };
        let mut entries = BTreeMap::new();
        for line in lines {
            let parts: Vec<&str> = line.split(' ').collect();
            if parts.len() != 5 || parts[0] != "entry" {
                return Err(bad());
            }
            let p = |s: &str| Digest::from_hex(s).ok_or_else(bad);
            let rec = EntryRecord {
                key: p(parts[1])?,
                lesson: p(parts[2])?,
                certificate: p(parts[3])?,
                object: p(parts[4])?,
            };
            if entries.insert(rec.key, rec).is_some() {
                return Err(bad());
            }
        }
        Ok((version, parent, entries))
    }

    pub fn snapshot(&self) -> Result<Snapshot, StoreError> {
        let (version, manifest) = self.read_head()?;
        let entries = match manifest {
            None => BTreeMap::new(),
            Some(m) => {
                let (v, _, e) = self.read_manifest(&m)?;
                if v != version {
                    return Err(StoreError::Corrupt("HEAD/manifest version mismatch".into()));
                }
                e
            }
        };
        Ok(Snapshot {
            version,
            manifest,
            entries,
        })
    }

    pub fn get(&self, key: &Digest) -> Result<Option<Vec<u8>>, StoreError> {
        let snap = self.snapshot()?;
        match snap.entries.get(key) {
            None => Ok(None),
            Some(rec) => Ok(Some(self.read_object(&rec.object)?)),
        }
    }

    fn read_object(&self, d: &Digest) -> Result<Vec<u8>, StoreError> {
        let bytes = fs::read(self.root.join("objects").join(d.to_hex()))?;
        if Digest::of(&bytes) != *d {
            return Err(StoreError::Corrupt(format!(
                "object {} hash mismatch",
                d.short()
            )));
        }
        Ok(bytes)
    }

    fn lock(&self) -> Result<LockGuard, StoreError> {
        match self.try_lock() {
            Err(StoreError::Busy) if self.recover_stale_lock()? => self.try_lock(),
            other => other,
        }
    }

    fn lock_waiting(&self, attempts: u32) -> Result<LockGuard, StoreError> {
        for i in 0..attempts.max(1) {
            match self.lock() {
                Err(StoreError::Busy) => {
                    if i % 8 == 7 {
                        std::thread::sleep(std::time::Duration::from_micros(200));
                    } else {
                        std::thread::yield_now();
                    }
                }
                other => return other,
            }
        }
        Err(StoreError::Busy)
    }

    /// Creates `LOCK` exclusively with its owner record already inside (the
    /// record is written to a private file first and hard-linked into place),
    /// so a crash can never leave an anonymous lock behind.
    fn try_lock(&self) -> Result<LockGuard, StoreError> {
        let path = self.root.join("LOCK");
        let owner = lock_owner_record();
        let tmp = self.root.join(format!(
            ".LOCK.{}.{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        {
            let mut f = File::create(&tmp)?;
            f.write_all(owner.as_bytes())?;
            f.sync_all()?;
        }
        let linked = fs::hard_link(&tmp, &path);
        let _ = fs::remove_file(&tmp);
        match linked {
            Ok(()) => Ok(LockGuard(path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(StoreError::Busy),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    /// Removes a `LOCK` whose owner process is provably dead (same host, pid no
    /// longer running). Returns whether a stale lock was removed. A lock whose
    /// owner cannot be judged (other host, unknown platform) is left alone and
    /// stays `Busy`: fail closed rather than risk two writers.
    pub fn recover_stale_lock(&self) -> Result<bool, StoreError> {
        let path = self.root.join("LOCK");
        let Ok(record) = fs::read_to_string(&path) else {
            return Ok(false);
        };
        if !owner_is_dead(&record) {
            return Ok(false);
        }
        // Move the stale lock aside atomically, then confirm it is the record
        // we judged; if another writer replaced it meanwhile, put it back.
        let aside = self.root.join(format!(
            "LOCK.stale.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        if fs::rename(&path, &aside).is_err() {
            return Ok(false);
        }
        let moved = fs::read_to_string(&aside).unwrap_or_default();
        if moved != record {
            let _ = fs::hard_link(&aside, &path);
            let _ = fs::remove_file(&aside);
            return Ok(false);
        }
        let _ = fs::remove_file(&aside);
        Ok(true)
    }

    fn write_object(&self, payload: &[u8], d: &Digest) -> Result<(), StoreError> {
        let dir = self.root.join("objects");
        let name = d.to_hex();
        if dir.join(&name).exists() {
            // Content-addressed: an existing object must already be identical.
            self.read_object(d)?;
            return Ok(());
        }
        write_atomic(&dir, &name, payload)?;
        Ok(())
    }

    fn write_manifest(
        &self,
        version: u64,
        parent: Option<Digest>,
        entries: &BTreeMap<Digest, EntryRecord>,
    ) -> Result<Digest, StoreError> {
        let mut text = format!(
            "{MANIFEST_MAGIC}\nversion {version}\nparent {}\n",
            parent.map_or("none".to_string(), |p| p.to_hex())
        );
        for r in entries.values() {
            text.push_str(&format!(
                "entry {} {} {} {}\n",
                r.key.to_hex(),
                r.lesson.to_hex(),
                r.certificate.to_hex(),
                r.object.to_hex()
            ));
        }
        let d = Digest::of(text.as_bytes());
        let dir = self.root.join("manifests");
        if !dir.join(d.to_hex()).exists() {
            write_atomic(&dir, &d.to_hex(), text.as_bytes())?;
        }
        Ok(d)
    }

    fn publish_head(&self, version: u64, manifest: Digest) -> Result<(), StoreError> {
        write_atomic(
            &self.root,
            "HEAD",
            format!("{HEAD_MAGIC}\n{version} {}\n", manifest.to_hex()).as_bytes(),
        )?;
        Ok(())
    }

    /// Atomically publishes a transaction (all entries or none).
    pub fn commit(&self, txn: &Transaction) -> Result<CommitOutcome, StoreError> {
        let _guard = self.lock()?;
        let snap = self.snapshot()?;
        if snap.version != txn.base_version {
            return Err(StoreError::Conflict {
                base: txn.base_version,
                current: snap.version,
            });
        }
        let mut entries = snap.entries.clone();
        let mut fresh: Vec<&StagedEntry> = Vec::new();
        let mut idempotent = 0;
        for staged in &txn.entries {
            let rec = staged.record;
            let existing = entries.get(&rec.key).copied().or_else(|| {
                fresh
                    .iter()
                    .find(|s| s.record.key == rec.key)
                    .map(|s| s.record)
            });
            match existing {
                Some(e) if e.object == rec.object => idempotent += 1,
                Some(e) => {
                    return Err(StoreError::Contradiction {
                        key: rec.key,
                        existing: e.object,
                        proposed: rec.object,
                    })
                }
                None => fresh.push(staged),
            }
        }
        if fresh.is_empty() {
            return Ok(CommitOutcome {
                version: snap.version,
                added: 0,
                idempotent,
            });
        }
        for s in &fresh {
            self.write_object(&s.payload, &s.record.object)?;
            entries.insert(s.record.key, s.record);
        }
        if self.fail_point == Some(FailPoint::AfterObjects) {
            return Err(StoreError::InjectedFailure(FailPoint::AfterObjects));
        }
        let version = snap.version + 1;
        let manifest = self.write_manifest(version, snap.manifest, &entries)?;
        if self.fail_point == Some(FailPoint::AfterManifest) {
            return Err(StoreError::InjectedFailure(FailPoint::AfterManifest));
        }
        self.publish_head(version, manifest)?;
        Ok(CommitOutcome {
            version,
            added: fresh.len(),
            idempotent,
        })
    }

    /// Commits, rebasing on conflict and waiting while another writer holds the
    /// lock. Contradictions and I/O errors are returned immediately.
    pub fn commit_rebasing(
        &self,
        entries: Vec<StagedEntry>,
        max_attempts: u32,
    ) -> Result<CommitOutcome, StoreError> {
        let mut last = StoreError::Busy;
        for attempt in 0..max_attempts.max(1) {
            let txn = Transaction {
                base_version: self.version()?,
                entries: entries.clone(),
            };
            match self.commit(&txn) {
                Ok(o) => return Ok(o),
                Err(e @ (StoreError::Busy | StoreError::Conflict { .. })) => {
                    last = e;
                    if attempt % 8 == 7 {
                        std::thread::sleep(std::time::Duration::from_micros(200));
                    } else {
                        std::thread::yield_now();
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// Versions reachable from HEAD, newest first.
    pub fn history(&self) -> Result<Vec<(u64, Digest)>, StoreError> {
        let mut out = Vec::new();
        let mut cur = self.read_head()?.1;
        while let Some(m) = cur {
            let (v, parent, _) = self.read_manifest(&m)?;
            out.push((v, m));
            cur = parent;
        }
        Ok(out)
    }

    /// Re-publishes the content of an earlier version as a new version.
    pub fn restore(&self, version: u64) -> Result<CommitOutcome, StoreError> {
        let _guard = self.lock()?;
        let (current, head) = self.read_head()?;
        let target = self
            .history()?
            .into_iter()
            .find(|(v, _)| *v == version)
            .ok_or(StoreError::UnknownVersion(version))?;
        let (_, _, entries) = self.read_manifest(&target.1)?;
        let new_version = current + 1;
        let manifest = self.write_manifest(new_version, head, &entries)?;
        self.publish_head(new_version, manifest)?;
        Ok(CommitOutcome {
            version: new_version,
            added: entries.len(),
            idempotent: 0,
        })
    }

    /// Exhaustive integrity audit of the whole history: every manifest and
    /// every referenced object is re-hashed. Returns the number of objects
    /// verified in the current version.
    pub fn audit(&self) -> Result<usize, StoreError> {
        let history = self.history()?;
        let mut expected = history.len() as u64;
        for (v, m) in &history {
            if *v != expected {
                return Err(StoreError::Corrupt(format!("non-contiguous version {v}")));
            }
            expected -= 1;
            let (_, _, entries) = self.read_manifest(m)?;
            for r in entries.values() {
                self.read_object(&r.object)?;
            }
        }
        Ok(self.snapshot()?.entries.len())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
pub(crate) fn test_certificate(seed: u64) -> Certificate {
    use crate::mastery::{certify, check_partitions, Cohort, CohortResult, ExamReport};
    let lesson = Digest::of(&seed.to_le_bytes());
    let c = |i: u64| Digest::of(&(seed * 16 + i).to_le_bytes());
    let corpus =
        check_partitions(&[c(1)], &[c(2)], &[c(3)], &[c(4)]).unwrap_or_else(|e| panic!("{e}"));
    let ok = |cohort| CohortResult {
        cohort,
        correct: 1,
        abstained: 0,
        total: 1,
    };
    let report = ExamReport {
        lesson,
        corpus,
        cohorts: vec![
            ok(Cohort::TrainRecall),
            ok(Cohort::Test),
            ok(Cohort::Reload),
        ],
    };
    certify(lesson, &report, &[]).unwrap_or_else(|e| panic!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "sml-tutor-store-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&p);
        p
    }

    fn entry(i: u64, payload: &[u8]) -> StagedEntry {
        StagedEntry::new(
            &test_certificate(i),
            Digest::of(&i.to_be_bytes()),
            payload.to_vec(),
        )
    }

    #[test]
    fn commit_idempotence_and_contradiction() {
        let dir = tmp("basic");
        let s = KnowledgeStore::open(&dir).unwrap();
        let o = s
            .commit(&Transaction {
                base_version: 0,
                entries: vec![entry(1, b"one"), entry(2, b"two")],
            })
            .unwrap();
        assert_eq!((o.version, o.added), (1, 2));
        let o = s
            .commit(&Transaction {
                base_version: 1,
                entries: vec![entry(1, b"one")],
            })
            .unwrap();
        assert_eq!((o.version, o.added, o.idempotent), (1, 0, 1));
        let err = s
            .commit(&Transaction {
                base_version: 1,
                entries: vec![entry(3, b"three"), entry(1, b"ONE")],
            })
            .unwrap_err();
        assert!(matches!(err, StoreError::Contradiction { .. }));
        // The contradiction aborted the whole transaction: key 3 is absent.
        assert_eq!(s.get(&Digest::of(&3u64.to_be_bytes())).unwrap(), None);
        assert_eq!(
            s.get(&Digest::of(&1u64.to_be_bytes())).unwrap().unwrap(),
            b"one"
        );
        assert!(matches!(
            s.commit(&Transaction {
                base_version: 0,
                entries: vec![entry(4, b"x")],
            }),
            Err(StoreError::Conflict {
                base: 0,
                current: 1
            })
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_before_head_rename_rolls_back() {
        for point in [FailPoint::AfterObjects, FailPoint::AfterManifest] {
            let dir = tmp(&format!("crash-{point:?}"));
            let mut s = KnowledgeStore::open(&dir).unwrap();
            s.commit(&Transaction {
                base_version: 0,
                entries: vec![entry(1, b"one")],
            })
            .unwrap();
            let before = s.snapshot().unwrap();
            s.inject_failure(Some(point));
            assert!(s
                .commit(&Transaction {
                    base_version: 1,
                    entries: vec![entry(2, b"two")],
                })
                .is_err());
            let reopened = KnowledgeStore::open(&dir).unwrap();
            assert_eq!(reopened.snapshot().unwrap(), before);
            assert_eq!(reopened.audit().unwrap(), 1);
            assert!(!dir.join("LOCK").exists(), "lock released after failure");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn concurrent_writers_lose_nothing() {
        let dir = tmp("concurrent");
        let s = Arc::new(KnowledgeStore::open(&dir).unwrap());
        std::thread::scope(|scope| {
            for t in 0..8u64 {
                let s = Arc::clone(&s);
                scope.spawn(move || {
                    for i in 0..10u64 {
                        let k = t * 100 + i;
                        s.commit_rebasing(vec![entry(k, &k.to_le_bytes())], 100_000)
                            .unwrap();
                    }
                });
            }
        });
        let snap = s.snapshot().unwrap();
        assert_eq!(snap.entries.len(), 80);
        assert_eq!(snap.version, 80);
        assert_eq!(s.audit().unwrap(), 80);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_lock_from_dead_process_is_recovered_live_lock_is_kept() {
        let dir = tmp("stale-lock");
        let s = KnowledgeStore::open(&dir).unwrap();
        // A lock left by a process that no longer exists on this host.
        fs::write(
            dir.join("LOCK"),
            format!("SML_TUTOR_LOCK_V1 4294967294 {}\n", host_id()),
        )
        .unwrap();
        if Path::new("/proc/self").exists() && !host_id().is_empty() {
            s.commit_rebasing(vec![entry(1, b"one")], 4).unwrap();
            assert!(!dir.join("LOCK").exists());
        }
        // A lock owned by a live process (this one) is never stolen.
        fs::write(dir.join("LOCK"), lock_owner_record()).unwrap();
        assert!(matches!(
            s.commit(&Transaction {
                base_version: s.version().unwrap(),
                entries: vec![entry(2, b"two")],
            }),
            Err(StoreError::Busy)
        ));
        assert!(!s.recover_stale_lock().unwrap());
        // A lock from another host cannot be judged: fail closed.
        fs::write(
            dir.join("LOCK"),
            "SML_TUTOR_LOCK_V1 4294967294 other-host\n",
        )
        .unwrap();
        assert!(!s.recover_stale_lock().unwrap());
        fs::remove_file(dir.join("LOCK")).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_first_open_never_overwrites_a_committed_head() {
        for round in 0..20 {
            let dir = tmp(&format!("first-open-{round}"));
            std::thread::scope(|scope| {
                for t in 0..4u64 {
                    let dir = dir.clone();
                    scope.spawn(move || {
                        let s = KnowledgeStore::open(&dir).unwrap();
                        s.commit_rebasing(vec![entry(t, &t.to_le_bytes())], 100_000)
                            .unwrap();
                    });
                }
            });
            let s = KnowledgeStore::open(&dir).unwrap();
            assert_eq!(s.snapshot().unwrap().entries.len(), 4, "round {round}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn history_restore_and_corruption_detection() {
        let dir = tmp("restore");
        let s = KnowledgeStore::open(&dir).unwrap();
        s.commit_rebasing(vec![entry(1, b"a")], 4).unwrap();
        s.commit_rebasing(vec![entry(2, b"b")], 4).unwrap();
        assert_eq!(s.history().unwrap().len(), 2);
        let o = s.restore(1).unwrap();
        assert_eq!(o.version, 3);
        assert_eq!(s.snapshot().unwrap().entries.len(), 1);
        assert!(matches!(s.restore(9), Err(StoreError::UnknownVersion(9))));
        // Tamper with an object: the exhaustive audit must fail.
        let obj = s
            .snapshot()
            .unwrap()
            .entries
            .values()
            .next()
            .unwrap()
            .object;
        fs::write(dir.join("objects").join(obj.to_hex()), b"tampered").unwrap();
        assert!(matches!(s.audit(), Err(StoreError::Corrupt(_))));
        let _ = fs::remove_dir_all(&dir);
    }
}
