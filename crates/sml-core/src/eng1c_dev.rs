//! Registered TRAIN/DEV identities and prefix-only source preparation.
//! The consumed source stops at DEV's end. No TEST data path or tail hash.
use crate::elastic_ngram_paged_sha::{hex, sha256};
use std::{error::Error, io::Read};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ROSTER: &str = include_str!("../../../scripts/eng1c/testtest-roster.tsv");
const MANIFEST: &str = include_str!("../../../scripts/eng1c/testtest-manifest.tsv");
pub const DEV_MANIFEST_SHA: &str =
    "ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990";

#[derive(Clone, Debug)]
pub struct DevCorpus {
    pub id: String,
    pub language: String,
    pub genre: String,
    pub author: String,
    pub title: String,
    pub raw_bytes: usize,
    pub train_bytes: usize,
    pub train_sha256: String,
    pub dev_start: usize,
    pub dev_bytes: usize,
    pub dev_sha256: String,
}
impl DevCorpus {
    pub fn size_stratum(&self) -> &'static str {
        if self.train_bytes < 262144 {
            "small"
        } else if self.train_bytes < 524288 {
            "medium"
        } else {
            "large"
        }
    }
    pub fn verify_splits(&self, train: &[u8], dev: &[u8]) -> Result<()> {
        if train.len() != self.train_bytes
            || dev.len() != self.dev_bytes
            || hex(&sha256(train)) != self.train_sha256
            || hex(&sha256(dev)) != self.dev_sha256
        {
            return Err("registered TRAIN/DEV identity mismatch".into());
        }
        Ok(())
    }
}
pub fn frozen_dev_corpora() -> Result<Vec<DevCorpus>> {
    if hex(&sha256(MANIFEST.as_bytes())) != DEV_MANIFEST_SHA {
        return Err("compiled manifest drift".into());
    }
    let roster: Vec<_> = ROSTER.lines().skip(1).collect();
    let manifest: Vec<_> = MANIFEST.lines().skip(1).collect();
    if roster.len() != 12 || manifest.len() != 12 {
        return Err("incomplete compiled corpus panel".into());
    }
    let mut result = Vec::new();
    for (r, m) in roster.iter().zip(manifest) {
        let r: Vec<_> = r.split('\t').collect();
        let m: Vec<_> = m.split('\t').collect();
        if r.len() != 6 || m.len() != 11 || r[0] != m[0] {
            return Err("compiled corpus row mismatch".into());
        }
        let n = m[3].parse::<usize>()?;
        let a = n.checked_mul(3).ok_or("split overflow")? / 5;
        let b = n.checked_mul(4).ok_or("split overflow")? / 5;
        let train_bytes = m[5].parse::<usize>()?;
        let dev_bytes = m[7].parse::<usize>()?;
        if train_bytes != a.min(600000) || dev_bytes != (b - a).min(32768) || dev_bytes < 4096 {
            return Err("compiled TRAIN/DEV split mismatch".into());
        }
        result.push(DevCorpus {
            id: r[0].into(),
            language: r[1].into(),
            genre: r[2].into(),
            author: r[3].into(),
            title: r[4].into(),
            raw_bytes: m[1].parse()?,
            train_bytes,
            train_sha256: m[6].into(),
            dev_start: a,
            dev_bytes,
            dev_sha256: m[8].into(),
        });
    }
    Ok(result)
}
pub fn frozen_dev_corpus(id: &str) -> Result<DevCorpus> {
    frozen_dev_corpora()?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "undeclared DEV corpus".into())
}

/// Byte-exact bounded reads. Header reads one byte at a time, with no
/// prefetch. Body requests at most the remaining normalized prefix length,
/// so CR stripping can never advance into the first normalized byte after DEV.
pub fn extract_train_dev(mut raw: impl Read, work: &DevCorpus) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut header = Vec::<String>::new();
    let mut header_bytes = 0;
    loop {
        let mut line = Vec::new();
        loop {
            let mut one = [0u8; 1];
            if raw.read(&mut one)? == 0 {
                return Err("source ended before body marker".into());
            }
            header_bytes += 1;
            if header_bytes > 65536 || line.len() > 4096 {
                return Err("source header budget exceeded".into());
            }
            if one[0] == b'\n' {
                break;
            }
            if one[0] != b'\r' {
                line.push(one[0]);
            }
        }
        let line = String::from_utf8(line)?;
        if line.starts_with("*** START OF THE PROJECT GUTENBERG EBOOK ") {
            break;
        }
        header.push(line);
    }
    let language = match work.language.as_str() {
        "FR" => "French",
        "EN" => "English",
        _ => return Err("unknown source language".into()),
    };
    for expected in [
        format!("Title: {}", work.title),
        format!("Author: {}", work.author),
        format!("Language: {language}"),
    ] {
        if header.iter().filter(|s| **s == expected).count() != 1 {
            return Err("raw source header identity mismatch".into());
        }
    }
    let end = work
        .dev_start
        .checked_add(work.dev_bytes)
        .ok_or("prefix overflow")?;
    if work.train_bytes > work.dev_start || end > 16 * 1024 * 1024 {
        return Err("invalid TRAIN/DEV prefix boundaries".into());
    }
    let mut train = Vec::with_capacity(work.train_bytes);
    let mut dev = Vec::with_capacity(work.dev_bytes);
    let mut position = 0;
    let mut buffer = [0u8; 65536];
    while position < end {
        let request = (end - position).min(buffer.len());
        let received = raw.read(&mut buffer[..request])?;
        if received == 0 {
            return Err("source ended before DEV boundary".into());
        }
        for byte in &buffer[..received] {
            if *byte == b'\r' {
                continue;
            }
            if position < work.train_bytes {
                train.push(*byte);
            }
            if position >= work.dev_start {
                dev.push(*byte);
            }
            position += 1;
        }
    }
    work.verify_splits(&train, &dev)?;
    Ok((train, dev))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_reader_never_requests_test_or_tail_even_with_crlf() {
        let body = "abc\n".repeat(8192);
        let bytes = body.as_bytes();
        let a = bytes.len() * 3 / 5;
        let b = bytes.len() * 4 / 5;
        let header="Title: Fixture\r\nAuthor: Fixture\r\nLanguage: French\r\n*** START OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n";
        let consumed = body[..b].replace('\n', "\r\n");
        let prefix = format!("{header}{consumed}").into_bytes();
        struct StopAtDev {
            data: Vec<u8>,
            pos: usize,
        }
        impl Read for StopAtDev {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.pos == self.data.len() {
                    return Err(std::io::Error::other("attempted TEST read"));
                }
                let n = out.len().min(self.data.len() - self.pos);
                out[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            }
        }
        let w = DevCorpus {
            id: "fixture".into(),
            language: "FR".into(),
            genre: "fixture".into(),
            author: "Fixture".into(),
            title: "Fixture".into(),
            raw_bytes: 0,
            train_bytes: a,
            train_sha256: hex(&sha256(&bytes[..a])),
            dev_start: a,
            dev_bytes: b - a,
            dev_sha256: hex(&sha256(&bytes[a..b])),
        };
        let (train, dev) = extract_train_dev(
            StopAtDev {
                data: prefix,
                pos: 0,
            },
            &w,
        )
        .unwrap();
        assert_eq!(train, &bytes[..a]);
        assert_eq!(dev, &bytes[a..b]);
        let mut corrupt = train.clone();
        corrupt[0] ^= 1;
        assert!(w.verify_splits(&corrupt, &dev).is_err());
        assert!(w.verify_splits(&dev, &train).is_err());
    }
    #[test]
    fn frozen_roster_and_size_bin_boundaries() {
        let rows = frozen_dev_corpora().unwrap();
        assert_eq!(rows.len(), 12);
        assert!(rows.iter().all(|r| r.dev_bytes == 32768));
        assert!(frozen_dev_corpus("800").is_err());
        let mut w = rows[0].clone();
        for (n, bin) in [
            (262143, "small"),
            (262144, "medium"),
            (524287, "medium"),
            (524288, "large"),
        ] {
            w.train_bytes = n;
            assert_eq!(w.size_stratum(), bin);
        }
    }
}
