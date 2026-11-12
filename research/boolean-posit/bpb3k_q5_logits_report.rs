//! Compare two source-pinned full-vocabulary debug dumps without pretending
//! independently generated sequences share context after their first
//! divergent greedy token. Research metrics, not a pass-quality gate.
use std::{fs, path::Path};

#[derive(Debug, PartialEq, Eq)]
enum Error {
    Truncated,
    BadMagic,
    InvalidShape,
    InvalidToken,
    NonFinite,
    TrailingBytes,
    PromptMismatch,
    VocabularyMismatch,
}
#[derive(Debug)]
struct Dump {
    vocab: usize,
    prompt: Vec<u32>,
    logits: Vec<Vec<f32>>,
    tokens: Vec<u32>,
}
fn word(bytes: &[u8], pos: &mut usize) -> Result<u32, Error> {
    let end = pos.checked_add(4).ok_or(Error::Truncated)?;
    let chunk = bytes.get(*pos..end).ok_or(Error::Truncated)?;
    *pos = end;
    Ok(u32::from_le_bytes(chunk.try_into().unwrap()))
}
fn parse(bytes: &[u8]) -> Result<Dump, Error> {
    if bytes.get(..8) != Some(b"BPB3FID2") {
        return Err(Error::BadMagic);
    }
    let mut pos = 8;
    let vocab = word(bytes, &mut pos)? as usize;
    let prompt_len = word(bytes, &mut pos)? as usize;
    if !(1..=1_000_000).contains(&vocab) || !(1..=4096).contains(&prompt_len) {
        return Err(Error::InvalidShape);
    }
    let prompt = (0..prompt_len)
        .map(|_| word(bytes, &mut pos))
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.iter().any(|&token| token as usize >= vocab) {
        return Err(Error::InvalidToken);
    }
    let steps = word(bytes, &mut pos)? as usize;
    if steps != 8 {
        return Err(Error::InvalidShape);
    }
    let mut logits = Vec::with_capacity(steps);
    let mut tokens = Vec::with_capacity(steps);
    for _ in 0..steps {
        let row = (0..vocab)
            .map(|_| word(bytes, &mut pos).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        if row.iter().any(|x| !x.is_finite()) {
            return Err(Error::NonFinite);
        }
        let selected = word(bytes, &mut pos)? as usize;
        if selected >= vocab {
            return Err(Error::InvalidToken);
        }
        let greedy = row.iter().enumerate()
            .max_by(|(ia, a), (ib, b)| a.total_cmp(b).then_with(|| ib.cmp(ia)))
            .map(|(index, _)| index).unwrap();
        if selected != greedy {
            return Err(Error::InvalidToken);
        }
        logits.push(row);
        tokens.push(selected as u32);
    }
    if pos != bytes.len() {
        return Err(Error::TrailingBytes);
    }
    Ok(Dump { vocab, prompt, logits, tokens })
}
fn report(a: &Dump, b: &Dump) -> Result<String, Error> {
    if a.vocab != b.vocab {
        return Err(Error::VocabularyMismatch);
    }
    if a.prompt != b.prompt {
        return Err(Error::PromptMismatch);
    }
    let mut csv = String::from("step,comparable_context,stock_token,q5_token,max_abs_logit_diff,rmse_logit_diff\n");
    let mut contexts_match = true;
    for step in 0..8 {
        if contexts_match {
            let mut max_error = 0.0f64;
            let mut sum_square = 0.0f64;
            for (base, candidate) in a.logits[step].iter().zip(&b.logits[step]) {
                let delta = (f64::from(*base) - f64::from(*candidate)).abs();
                max_error = max_error.max(delta);
                sum_square += delta * delta;
            }
            let rmse = (sum_square / a.vocab as f64).sqrt();
            csv.push_str(&format!(
                "{step},true,{},{},{max_error:.9},{rmse:.9}\n",
                a.tokens[step], b.tokens[step]
            ));
        } else {
            csv.push_str(&format!(
                "{step},false,{},{},NA,NA\n",
                a.tokens[step], b.tokens[step]
            ));
        }
        contexts_match &= a.tokens[step] == b.tokens[step];
    }
    Ok(csv)
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let result = (|| -> Result<String, String> {
        if arguments.len() != 3 {
            return Err("USAGE: q5-logit-report stock.bin q5.bin".into());
        }
        let stock = fs::read(Path::new(&arguments[1])).map_err(|e| e.to_string())?;
        let q5 = fs::read(Path::new(&arguments[2])).map_err(|e| e.to_string())?;
        let a = parse(&stock).map_err(|e| format!("STOCK_DUMP_{e:?}"))?;
        let b = parse(&q5).map_err(|e| format!("Q5_DUMP_{e:?}"))?;
        report(&a, &b).map_err(|e| format!("COMPARE_{e:?}"))
    })();
    match result {
        Ok(value) => print!("{value}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dump(tokens: [u32; 8]) -> Vec<u8> {
        let mut bytes = b"BPB3FID2".to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        for token in tokens {
            let row = (0..3usize).map(|i| {
                if i == token as usize { 2.0f32 } else { -1.0f32 }
            });
            for x in row {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
            bytes.extend_from_slice(&token.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn disallows_numeric_comparison_after_greedy_context_divergence() {
        let stock = parse(&dump([0, 1, 2, 0, 0, 0, 0, 0])).unwrap();
        let q5 = parse(&dump([0, 2, 1, 1, 1, 1, 1, 1])).unwrap();
        let csv = report(&stock, &q5).unwrap();
        let lines: Vec<_> = csv.lines().collect();
        assert!(lines[1].starts_with("0,true,0,0,"));
        assert!(lines[2].starts_with("1,true,1,2,"));
        assert!(lines[3].starts_with("2,false,2,1,NA,NA"));
        assert_eq!(lines.len(), 9);
    }

    #[test]
    fn rejects_invalid_contexts_and_numerics() {
        let mut bytes = dump([0; 8]);
        let valid = parse(&bytes).unwrap();
        assert_eq!(valid.logits.len(), 8);
        let last = bytes.len() - 4;
        bytes[last..].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(parse(&bytes).unwrap_err(), Error::InvalidToken);
        let mut altered = dump([0; 8]);
        altered[24..28].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(parse(&altered).unwrap_err(), Error::InvalidShape);
        let mut altered = dump([0; 8]);
        altered.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(parse(&altered).unwrap_err(), Error::TrailingBytes);
        let short = dump([0; 8]);
        assert_eq!(parse(&short[..short.len() - 1]).unwrap_err(), Error::Truncated);
        let mut altered = dump([0; 8]);
        // Logit[0] at offset: magic8+vocab4+prompt_count4+prompt8+steps4.
        altered[28..32].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(parse(&altered).unwrap_err(), Error::NonFinite);
        let mut b = dump([0; 8]);
        // Same shape but different prompt.
        b[20..24].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            report(&parse(&dump([0; 8])).unwrap(), &parse(&b).unwrap()).unwrap_err(),
            Error::PromptMismatch
        );
    }
}
