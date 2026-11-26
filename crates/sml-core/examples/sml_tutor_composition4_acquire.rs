//! Acquire a selected four-input truth table from external labels only.
//! The diagnostic signature is an admission oracle, never a fitting input.
//! Exhaustive replay covers the same 16 addresses: no unseen-data claim.
//! Writes an isolated candidate only; durable publication is external.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs, io::Read, io::Write};

const MARKER: &str = "SML_TUTOR_COMPOSITION4_GATE_V1";
const WIDTH: usize = 4;
const LIMIT: u64 = 256;

fn bits(address: usize) -> [bool; WIDTH] {
    [
        address & 1 != 0,
        address & 2 != 0,
        address & 4 != 0,
        address & 8 != 0,
    ]
}

fn parse_signature(raw: &str) -> Result<u16, Box<dyn Error>> {
    let signature = u16::from_str_radix(raw, 16)?;
    if format!("{signature:04x}") != raw || signature & 1 != 0 {
        return Err("noncanonical or non-zero-preserving signature".into());
    }
    for bit in 0..WIDTH {
        let mask = 1 << bit;
        if !(0..16).any(|i| (signature >> i) & 1 != (signature >> (i ^ mask)) & 1) {
            return Err("signature does not depend on every input".into());
        }
    }
    Ok(signature)
}

fn parse_train(raw: &str) -> Result<Vec<bool>, Box<dyn Error>> {
    if raw.len() as u64 > LIMIT {
        return Err("oversized TRAIN".into());
    }
    let mut rows = [None; 16];
    for line in raw.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [a, b, c, d, y] = fields.as_slice() else {
            return Err("expected a|b|c|d|y".into());
        };
        let bit = |s: &str| match s {
            "0" => Ok(0_usize),
            "1" => Ok(1_usize),
            _ => Err("non-binary TRAIN"),
        };
        let address = bit(a)? | (bit(b)? << 1) | (bit(c)? << 2) | (bit(d)? << 3);
        if rows[address].replace(bit(y)? != 0).is_some() {
            return Err("duplicate TRAIN address".into());
        }
    }
    rows.into_iter()
        .map(|row| row.ok_or_else(|| "missing TRAIN address".into()))
        .collect()
}

fn encode(gate: &LocalAnfGate, signature: u16) -> String {
    let word = (0..gate.monomial_count()).fold(0_u64, |word, m| {
        word | (u64::from(gate.coefficient(m)) << m)
    });
    format!("{MARKER}\n{signature:04x}\n0,1,2,3\n{word}\n")
}

fn restore(raw: &str, signature: u16) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() as u64 > LIMIT
        || lines.len() != 4
        || lines[0] != MARKER
        || parse_signature(lines[1])? != signature
        || lines[2] != "0,1,2,3"
    {
        return Err("checkpoint schema or signature mismatch".into());
    }
    let gate = LocalAnfGate::new(vec![0, 1, 2, 3], vec![lines[3].parse()?], WIDTH)?;
    if encode(&gate, signature) != raw {
        return Err("noncanonical checkpoint".into());
    }
    Ok(gate)
}

fn verify(gate: &LocalAnfGate, signature: u16) -> Result<(), Box<dyn Error>> {
    for address in 0..16 {
        if gate.evaluate(&bits(address)) != (signature & (1 << address) != 0) {
            return Err("scalar replay veto".into());
        }
    }
    // Four different permutations of the full domain, not new examples.
    let mut source = [0_u64; WIDTH];
    let mut expected = 0_u64;
    for lane in 0..64 {
        let address = ((lane % 16) * (2 * (lane / 16) + 1) + lane / 16) % 16;
        for (i, value) in bits(address).into_iter().enumerate() {
            source[i] |= u64::from(value) << lane;
        }
        expected |= u64::from(signature & (1 << address) != 0) << lane;
    }
    if gate.evaluate_lanes(&source, &mut [0_u64; 16]) != expected {
        return Err("bit-sliced replay veto".into());
    }
    Ok(())
}

fn qualify(table: &[bool], signature: u16) -> Result<String, Box<dyn Error>> {
    let gate = LocalAnfGate::from_truth_table(vec![0, 1, 2, 3], table, WIDTH)?;
    verify(&gate, signature)?;
    let checkpoint = encode(&gate, signature);
    let restored = restore(&checkpoint, signature)?;
    verify(&restored, signature)?;
    if gate != restored {
        return Err("coefficient replay mismatch".into());
    }
    Ok(checkpoint)
}

fn read_bounded(path: &str) -> Result<String, Box<dyn Error>> {
    let mut raw = String::new();
    fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_string(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err("oversized input".into());
    }
    Ok(raw)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [mode, signature, path] if mode == "--verify" => {
            let target = parse_signature(signature)?;
            verify(&restore(&read_bounded(path)?, target)?, target)?;
            println!(
                "SML_TUTOR_COMPOSITION4_RELOAD=PASS signature={signature} scalar=16 lanes=64 errors=0"
            );
        }
        [signature, train, path] => {
            let target = parse_signature(signature)?;
            let table = parse_train(&read_bounded(train)?)?;
            let checkpoint = qualify(&table, target)?;
            let gate = restore(&checkpoint, target)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(checkpoint.as_bytes())?;
            file.sync_all()?;
            println!(
                "SML_TUTOR_COMPOSITION4_LEARNED=PASS signature={signature} train_rows=16 monomials={} scalar=16 lanes=64 errors=0",
                gate.nonzero_monomials()
            );
        }
        _ => return Err("usage: SIGNATURE TRAIN NEW_CHECKPOINT | --verify SIGNATURE CHECKPOINT".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(signature: u16) -> String {
        (0..16)
            .map(|i| {
                let [a, b, c, d] = bits(i).map(u8::from);
                let y = (signature >> i) & 1;
                format!("{a}|{b}|{c}|{d}|{y}\n")
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn two_distinct_signatures_fit_and_reload() {
        for target in [0x099e, 0x8000] {
            let table = parse_train(&fixture(target)).unwrap();
            let checkpoint = qualify(&table, target).unwrap();
            let gate = restore(&checkpoint, target).unwrap();
            verify(&gate, target).unwrap();
            assert!(restore(&checkpoint, target ^ 2).is_err());
        }
        let raw = qualify(&parse_train(&fixture(0x099e)).unwrap(), 0x099e).unwrap();
        assert_eq!(restore(&raw, 0x099e).unwrap().nonzero_monomials(), 10);
    }

    #[test]
    fn wrong_labels_and_incomplete_train_rejected() {
        let mut table = parse_train(&fixture(0x099e)).unwrap();
        table[7] = !table[7];
        assert!(qualify(&table, 0x099e).is_err());
        assert!(parse_train("0|0|0|0|0\n").is_err());
        assert!(parse_train(&format!("{}0|0|0|0|0\n", fixture(0x099e))).is_err());
        assert!(parse_train("0|0|0|0|2\n").is_err());
    }

    #[test]
    fn invalid_signature_and_corrupted_checkpoint_rejected() {
        for invalid in ["099E", "99e", "099f", "aaaa", "0000", "10000"] {
            assert!(parse_signature(invalid).is_err());
        }
        let raw = format!("{MARKER}\n099e\n0,1,2,3\n0\n");
        assert!(verify(&restore(&raw, 0x099e).unwrap(), 0x099e).is_err());
        assert!(restore(&format!("{raw}\n"), 0x099e).is_err());
    }
}
