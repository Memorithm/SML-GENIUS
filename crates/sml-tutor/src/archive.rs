//! Lossless, verified archive of a finished cycle directory.
//!
//! A finished cycle store is many small files (one per object), so most of
//! its disk use is block and inode overhead. `pack` writes every regular
//! file of a directory into one self-describing file and returns its
//! digest; `verify_against` re-reads the archive and checks it byte for byte
//! against the directory before anything is removed; `unpack` restores the
//! exact tree. Nothing is summarised or dropped: the archive holds every
//! byte of every file.
//!
//! Format `SML_TUTOR_PACK_V1` (all integers little-endian):
//!
//! ```text
//! magic  b"SMLPACK1"
//! count  u64
//! count × { path_len u64, path (UTF-8, '/'-separated, relative),
//!           size u64, sha256 [32], bytes }
//! trailer sha256 [32] of everything before it
//! ```
//!
//! Symlinks, special files, absolute paths and `..` components are refused
//! (fail closed), on packing and on unpacking.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::hash::Digest;
use sml_core::elastic_ngram_paged_sha::Sha256;

const MAGIC: &[u8; 8] = b"SMLPACK1";
/// Upper bound on a single member, far above any cycle object.
const MAX_MEMBER: u64 = 1 << 30;
const MAX_PATH: u64 = 4096;

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    Refused(String),
    Corrupt(String),
}

impl core::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArchiveError::Io(e) => write!(f, "archive i/o: {e}"),
            ArchiveError::Refused(r) => write!(f, "archive refused: {r}"),
            ArchiveError::Corrupt(r) => write!(f, "archive corrupt: {r}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self {
        ArchiveError::Io(e)
    }
}

/// Relative paths of every regular file under `dir`, sorted.
fn members(dir: &Path) -> Result<Vec<String>, ArchiveError> {
    fn walk(base: &Path, rel: &Path, out: &mut Vec<String>) -> Result<(), ArchiveError> {
        for entry in fs::read_dir(base.join(rel))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| ArchiveError::Refused("non UTF-8 file name".into()))?;
            let r = rel.join(name);
            let t = entry.file_type()?;
            if t.is_symlink() {
                return Err(ArchiveError::Refused(format!("symlink {}", r.display())));
            } else if t.is_dir() {
                walk(base, &r, out)?;
            } else if t.is_file() {
                let s = r
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(s);
            } else {
                return Err(ArchiveError::Refused(format!(
                    "special file {}",
                    r.display()
                )));
            }
        }
        Ok(())
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Err(ArchiveError::Refused("source is not a directory".into()));
    }
    let mut out = Vec::new();
    walk(dir, Path::new(""), &mut out)?;
    out.sort();
    Ok(out)
}

fn safe_rel(path: &str) -> Result<PathBuf, ArchiveError> {
    let p = Path::new(path);
    if path.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(ArchiveError::Refused(format!(
            "unsafe member path {path:?}"
        )));
    }
    Ok(p.to_path_buf())
}

struct Hashing<W> {
    inner: W,
    sha: Sha256,
}

impl<W: Write> Hashing<W> {
    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.sha.update(bytes);
        self.inner.write_all(bytes)
    }
}

/// Writes `dir` into the new file `out` (which must not exist), fsyncs it
/// and returns the archive digest (the trailer).
pub fn pack(dir: &Path, out: &Path) -> Result<Digest, ArchiveError> {
    let names = members(dir)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)?;
    let mut w = Hashing {
        inner: io::BufWriter::new(file),
        sha: Sha256::default(),
    };
    w.put(MAGIC)?;
    w.put(&(names.len() as u64).to_le_bytes())?;
    for name in &names {
        let bytes = fs::read(dir.join(safe_rel(name)?))?;
        w.put(&(name.len() as u64).to_le_bytes())?;
        w.put(name.as_bytes())?;
        w.put(&(bytes.len() as u64).to_le_bytes())?;
        w.put(&Digest::of(&bytes).0)?;
        w.put(&bytes)?;
    }
    let trailer = Digest(w.sha.finish());
    w.inner.write_all(&trailer.0)?;
    let file = w.inner.into_inner().map_err(|e| e.into_error())?;
    file.sync_all()?;
    Ok(trailer)
}

/// The members of an archive: (relative path, file bytes), sorted by path.
pub type Members = Vec<(String, Vec<u8>)>;

/// Every member of an archive, after full integrity checks.
pub fn read(archive: &Path) -> Result<(Digest, Members), ArchiveError> {
    let mut data = Vec::new();
    File::open(archive)?.read_to_end(&mut data)?;
    if data.len() < MAGIC.len() + 8 + 32 || &data[..8] != MAGIC {
        return Err(ArchiveError::Corrupt("bad magic or truncated".into()));
    }
    let (body, trailer) = data.split_at(data.len() - 32);
    if Digest::of(body).0 != trailer {
        return Err(ArchiveError::Corrupt("trailer digest mismatch".into()));
    }
    let mut pos = 8usize;
    let take = |pos: &mut usize, n: u64| -> Result<&[u8], ArchiveError> {
        let n = usize::try_from(n).map_err(|_| ArchiveError::Corrupt("length".into()))?;
        let end = pos
            .checked_add(n)
            .filter(|&e| e <= body.len())
            .ok_or_else(|| ArchiveError::Corrupt("truncated member".into()))?;
        let s = &body[*pos..end];
        *pos = end;
        Ok(s)
    };
    let u64_at = |pos: &mut usize| -> Result<u64, ArchiveError> {
        let b = take(pos, 8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    };
    let count = u64_at(&mut pos)?;
    let mut out = Vec::new();
    let mut last: Option<String> = None;
    for _ in 0..count {
        let plen = u64_at(&mut pos)?;
        if plen > MAX_PATH {
            return Err(ArchiveError::Corrupt("path too long".into()));
        }
        let name = std::str::from_utf8(take(&mut pos, plen)?)
            .map_err(|_| ArchiveError::Corrupt("path not UTF-8".into()))?
            .to_string();
        safe_rel(&name)?;
        if last.as_ref().is_some_and(|l| *l >= name) {
            return Err(ArchiveError::Corrupt("members not strictly sorted".into()));
        }
        let size = u64_at(&mut pos)?;
        if size > MAX_MEMBER {
            return Err(ArchiveError::Corrupt("member too large".into()));
        }
        let want: [u8; 32] = take(&mut pos, 32)?.try_into().expect("32 bytes");
        let bytes = take(&mut pos, size)?.to_vec();
        if Digest::of(&bytes).0 != want {
            return Err(ArchiveError::Corrupt(format!(
                "member {name} digest mismatch"
            )));
        }
        last = Some(name.clone());
        out.push((name, bytes));
    }
    if pos != body.len() {
        return Err(ArchiveError::Corrupt("trailing bytes".into()));
    }
    Ok((Digest(trailer.try_into().expect("32 bytes")), out))
}

/// Checks that `archive` holds exactly the regular files of `dir`, byte for
/// byte, and that its trailer equals `digest`.
pub fn verify_against(archive: &Path, digest: Digest, dir: &Path) -> Result<usize, ArchiveError> {
    let (trailer, files) = read(archive)?;
    if trailer != digest {
        return Err(ArchiveError::Corrupt(
            "archive digest differs from the packed one".into(),
        ));
    }
    let names = members(dir)?;
    if names.len() != files.len() {
        return Err(ArchiveError::Corrupt(
            "member count differs from the directory".into(),
        ));
    }
    for (name, (aname, bytes)) in names.iter().zip(&files) {
        if name != aname || fs::read(dir.join(safe_rel(name)?))? != *bytes {
            return Err(ArchiveError::Corrupt(format!(
                "member {name} differs from disk"
            )));
        }
    }
    Ok(files.len())
}

/// Restores an archive into `dest`, which must not exist yet.
pub fn unpack(archive: &Path, dest: &Path) -> Result<usize, ArchiveError> {
    let (_, files) = read(archive)?;
    if fs::symlink_metadata(dest).is_ok() {
        return Err(ArchiveError::Refused("destination exists".into()));
    }
    fs::create_dir_all(dest)?;
    for (name, bytes) in &files {
        let path = dest.join(safe_rel(name)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "sml-tutor-archive-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("store/objects")).unwrap();
        fs::write(root.join("journal.log"), b"HEADER\nline 1\n").unwrap();
        fs::write(root.join("store/HEAD"), b"abc\n").unwrap();
        fs::write(root.join("store/objects/00ff"), vec![7u8; 5000]).unwrap();
        fs::write(root.join("store/objects/empty"), b"").unwrap();
    }

    #[test]
    fn pack_verify_unpack_roundtrip_is_byte_identical() {
        let t = tmp("roundtrip");
        let src = t.join("src");
        tree(&src);
        let a = t.join("cycle.smlpack");
        let d = pack(&src, &a).unwrap();
        assert_eq!(verify_against(&a, d, &src).unwrap(), 4);
        assert!(
            pack(&src, &a).is_err(),
            "existing archive is never overwritten"
        );
        let back = t.join("back");
        assert_eq!(unpack(&a, &back).unwrap(), 4);
        assert_eq!(members(&back).unwrap(), members(&src).unwrap());
        for m in members(&src).unwrap() {
            assert_eq!(
                fs::read(src.join(&m)).unwrap(),
                fs::read(back.join(&m)).unwrap()
            );
        }
        assert!(
            unpack(&a, &back).is_err(),
            "existing destination is refused"
        );
    }

    #[test]
    fn any_flipped_byte_or_truncation_is_detected() {
        let t = tmp("tamper");
        let src = t.join("src");
        tree(&src);
        let a = t.join("cycle.smlpack");
        let d = pack(&src, &a).unwrap();
        let good = fs::read(&a).unwrap();
        for i in [0, 9, 20, good.len() / 2, good.len() - 1] {
            let mut bad = good.clone();
            bad[i] ^= 1;
            fs::write(&a, &bad).unwrap();
            assert!(read(&a).is_err(), "flip at {i} not detected");
        }
        fs::write(&a, &good[..good.len() - 1]).unwrap();
        assert!(read(&a).is_err());
        fs::write(&a, &good).unwrap();
        fs::write(src.join("store/HEAD"), b"abd\n").unwrap();
        assert!(
            verify_against(&a, d, &src).is_err(),
            "disk change is detected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_refused() {
        let t = tmp("symlink");
        let src = t.join("src");
        tree(&src);
        std::os::unix::fs::symlink("/etc/passwd", src.join("link")).unwrap();
        assert!(matches!(
            pack(&src, &t.join("a")),
            Err(ArchiveError::Refused(_))
        ));
    }

    #[test]
    fn unsafe_paths_are_refused() {
        for p in ["", "/etc/passwd", "../x", "a/../../x", "./a"] {
            assert!(safe_rel(p).is_err(), "{p:?} accepted");
        }
        assert!(safe_rel("store/objects/ab").is_ok());
    }
}
