//! A3 Development diagnostic: extra observed history, NOT improved latent memory.
//! The unchanged A2 executable is rerun and its complete frozen output verified.

use std::collections::{BTreeMap, BTreeSet};
use std::{error::Error, fmt::Write as _};

use sml_core::context_ledger::{
    ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger,
};
use sml_store::observed_history::{
    encode_event, screen_candidate, verify_history, HistoryLimits, HistoryVerificationLimits,
    ObservationEvent, ObservedState, SubjectKey, MAX_EVENT_BYTES,
};
use sml_store::raw_context::sha256_raw_bytes;

#[allow(dead_code)]
#[path = "dkv_projection_contrast_a2.rs"]
mod a2;

type R<T> = Result<T, Box<dyn Error>>;
const PROTOCOL: &str = "38664b871cde7cea7e507725ceff9a1ab4597ccd";
const A2_HASH: &str = "084b3de117f5ad1859528e1dce237618fa1bdd1e8bea9fec875783a042e04448";
const RETAINED: &str = include_str!("../../../research/dkv-r1a2/development.csv");
const ARMS: [&str; 3] = ["standard", "contrast", "direct"];
const FAMILIES: [&str; 8] = [
    "isolated",
    "old",
    "recent",
    "overwrite",
    "conflict",
    "missing",
    "near_old",
    "near_recent",
];

fn require(ok: bool, message: &str) -> R<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn debug_error(error: impl std::fmt::Debug) -> Box<dyn Error> {
    format!("{error:?}").into()
}

/// Independent source generator. It never receives a latent or gold output.
pub fn source_events(family: &str, payload: u8, group: u32) -> R<Vec<ObservationEvent>> {
    require(payload < 4 && group < 2, "source identity bounds")?;
    let target = [payload & 1 != 0, payload & 2 != 0];
    let opposite = [!target[0], !target[1]];
    let mut records = Vec::new();
    let distractors = || (2..=8).map(|subject| (subject, opposite, None));
    match family {
        "isolated" => records.push((1, target, None)),
        "old" | "near_old" => {
            records.push((1, target, None));
            records.extend(distractors());
        }
        "recent" | "near_recent" => {
            records.extend(distractors());
            records.push((1, target, None));
        }
        "overwrite" => {
            records.push((1, opposite, None));
            records.extend(distractors());
            records.push((1, target, Some(1)));
        }
        "conflict" => {
            records.push((1, opposite, None));
            records.push((1, target, None));
        }
        "missing" => records.extend(distractors()),
        _ => return Err("unknown source family".into()),
    }
    records
        .into_iter()
        .enumerate()
        .map(|(index, (subject, bits, predecessor))| {
            let key = SubjectKey::new(subject, group)?;
            Ok(ObservationEvent::new(
                index as u64 + 1,
                key,
                bits,
                predecessor,
            )?)
        })
        .collect()
}

fn raw_snapshot(events: &[ObservationEvent]) -> R<(RawContextLedger, Vec<Vec<u8>>)> {
    require(events.len() <= 16, "event limit")?;
    let stream = ContextStreamId::new(1).map_err(debug_error)?;
    let mut ledger = RawContextLedger::new(stream);
    let mut sources = Vec::new();
    for (index, &event) in events.iter().enumerate() {
        let raw = encode_event(event);
        let id = ContextSegmentId::new(stream, index as u64 + 1).map_err(debug_error)?;
        let reference = ContextSegmentRef::new(
            id,
            ledger.stats().raw_bytes,
            raw.len() as u64,
            0,
            0,
            index as u64,
            sha256_raw_bytes(&raw)?,
            vec![event.id()],
        )
        .map_err(debug_error)?;
        ledger.append(reference).map_err(debug_error)?;
        sources.push(raw);
    }
    Ok((ledger, sources))
}

fn bits(text: &str) -> R<Option<[bool; 2]>> {
    match text {
        "00" => Ok(Some([false, false])),
        "01" => Ok(Some([false, true])),
        "10" => Ok(Some([true, false])),
        "11" => Ok(Some([true, true])),
        "unknown" => Ok(None),
        _ => Err("noncanonical candidate output".into()),
    }
}

fn bit_text(value: Option<[bool; 2]>) -> String {
    value.map_or_else(
        || String::from("unknown"),
        |v| format!("{}{}", u8::from(v[0]), u8::from(v[1])),
    )
}

fn state_text(state: ObservedState) -> &'static str {
    match state {
        ObservedState::Missing => "missing",
        ObservedState::Agreement => "agreement",
        ObservedState::Conflict => "conflict",
    }
}

/// Evaluation only. This function does not construct or screen candidates.
fn partition(value: Option<[bool; 2]>, gold: Option<[bool; 2]>) -> [u64; 4] {
    match (value.is_some(), value == gold) {
        (true, true) => [1, 0, 0, 0],
        (true, false) => [0, 1, 0, 0],
        (false, true) => [0, 0, 1, 0],
        (false, false) => [0, 0, 0, 1],
    }
}

#[derive(Default)]
struct Counts {
    rows: u64,
    latent_correct: u64,
    partitions: [u64; 4],
    suppressed: u64,
    segments: u64,
    bytes: u64,
}

impl Counts {
    fn observe(
        &mut self,
        candidate: Option<[bool; 2]>,
        screened: Option<[bool; 2]>,
        gold: Option<[bool; 2]>,
        segments: u64,
        bytes: u64,
    ) {
        self.rows += 1;
        self.latent_correct += u64::from(candidate == gold);
        for (count, increment) in self.partitions.iter_mut().zip(partition(screened, gold)) {
            *count += increment;
        }
        self.suppressed += u64::from(candidate.is_some() && screened.is_none());
        self.segments += segments;
        self.bytes += bytes;
    }

    fn line(&self, prefix: &str) -> String {
        let [ck, wk, cu, wu] = self.partitions;
        format!(
            "{prefix},rows={},latent_correct={},correct_known={ck},wrong_known={wk},correct_unknown={cu},wrong_unknown={wu},suppressed={},source_segments={},source_bytes={}",
            self.rows, self.latent_correct, self.suppressed, self.segments, self.bytes
        )
    }
}

pub struct Diagnostic {
    pub csv: String,
    pub summary: String,
    /// Compact copy of actual A2 latent observations, in A2 row order.
    pub packet: String,
}

pub fn diagnose(a2_text: &str) -> R<Diagnostic> {
    require(a2_text.len() <= 16 * 1024 * 1024, "input report limit")?;
    require(a2_text == RETAINED, "A2 frozen bytes changed")?;
    require(
        sha256_raw_bytes(a2_text.as_bytes())?.hex() == A2_HASH,
        "A2 hash",
    )?;
    let mut csv = format!(
        "SCHEMA,sml.dkv-a3.observed-screen.v1\nPROTOCOL,{PROTOCOL}\nA2_OUTPUT_SHA256,{A2_HASH}\nSTATUS,DEVELOPMENT_ONLY\n"
    );
    csv.push_str("A3_ROW_HEADER,dimension,seed,group,payload,family,arm,gold,latent,observed_state,screening_reason,screened,latent_correct,correct_known,wrong_known,correct_unknown,wrong_unknown,source_segments,source_bytes\n");
    let mut packet = String::new();
    let mut identities = BTreeSet::new();
    let mut totals: BTreeMap<String, Counts> = BTreeMap::new();
    let mut families: BTreeMap<(String, String), Counts> = BTreeMap::new();
    for row in a2_text.lines().filter(|line| line.starts_with("ROW,")) {
        let fields: Vec<_> = row.split(',').collect();
        require(fields.len() == 22, "A2 row schema")?;
        let dimension: u32 = fields[1].parse()?;
        let seed: u32 = fields[2].parse()?;
        let group: u32 = fields[3].parse()?;
        let payload: u8 = fields[4].parse()?;
        let family = fields[5];
        let arm = fields[6];
        require(
            [32, 64, 128].contains(&dimension) && seed < 3,
            "grid bounds",
        )?;
        require(
            ARMS.contains(&arm) && FAMILIES.contains(&family),
            "grid identity",
        )?;
        let identity = fields[1..7].join(",");
        require(identities.insert(identity.clone()), "duplicate paired row")?;
        // Only the latent column becomes the model candidate.
        let candidate = bits(fields[8])?;
        let events = source_events(family, payload, group)?;
        let (ledger, raw) = raw_snapshot(&events)?;
        let history = verify_history(
            &ledger,
            HistoryLimits {
                max_events: 16,
                max_keys: 8,
                max_element_bytes: 8192,
            },
            HistoryVerificationLimits {
                max_segments: 16,
                max_raw_bytes: 4096,
                max_record_bytes: MAX_EVENT_BYTES,
            },
            |reference| Ok(raw[(reference.identity.segment_id - 1) as usize].clone()),
        )?;
        let key = SubjectKey::new(1, group)?;
        let state = history.query(key).summary().state;
        let result = screen_candidate(&history, key, candidate);
        let screened = result.value();
        require(result.original() == candidate, "original candidate changed")?;
        if state == ObservedState::Agreement {
            require(
                screened == candidate,
                "agreement must not repair a candidate",
            )?;
        } else {
            require(
                screened.is_none(),
                "missing/conflicting history must suppress",
            )?;
        }
        // Evaluator gold is consulted only AFTER screening completes.
        let gold = bits(fields[7])?;
        let [ck, wk, cu, wu] = partition(screened, gold);
        let latent_correct = u8::from(candidate == gold);
        let gold_text = bit_text(gold);
        let latent_text = bit_text(candidate);
        let screened_text = bit_text(screened);
        let observed = state_text(state);
        let reason = result.reason();
        let segments = history.generation() as u64;
        let bytes = history.raw_bytes();
        writeln!(csv, "A3_ROW,{identity},{gold_text},{latent_text},{observed},{reason:?},{screened_text},{latent_correct},{ck},{wk},{cu},{wu},{segments},{bytes}")?;
        packet.push(match candidate {
            None => 'x',
            Some(v) => char::from(b'0' + u8::from(v[0]) + 2 * u8::from(v[1])),
        });
        totals
            .entry(arm.to_string())
            .or_default()
            .observe(candidate, screened, gold, segments, bytes);
        families
            .entry((arm.to_string(), family.to_string()))
            .or_default()
            .observe(candidate, screened, gold, segments, bytes);
        require(csv.len() <= 2 * 1024 * 1024, "A3 report limit")?;
    }
    require(
        identities.len() == 1728 && packet.len() == 1728,
        "complete paired panel",
    )?;
    let mut summary = String::new();
    for arm in ARMS {
        let counts = totals.get(arm).ok_or("missing arm")?;
        require(counts.rows == 576, "arm row count")?;
        require(
            counts.partitions.iter().sum::<u64>() == counts.rows,
            "outcome partition",
        )?;
        writeln!(summary, "{}", counts.line(&format!("A3_SUMMARY,{arm}")))?;
        for family in FAMILIES {
            let counts = families
                .get(&(arm.to_string(), family.to_string()))
                .ok_or("missing family")?;
            require(counts.rows == 72, "family row count")?;
            writeln!(
                summary,
                "{}",
                counts.line(&format!("A3_FAMILY,{arm},{family}"))
            )?;
        }
    }
    csv.push_str(&summary);
    csv.push_str("SOURCE_COST,complete_observed_history_is_additional_information_and_work\n");
    csv.push_str("PROMOTION,BLOCKED,source_screen_is_not_latent_or_full_context_qualification\n");
    Ok(Diagnostic {
        csv,
        summary,
        packet,
    })
}

pub fn run() -> R<Diagnostic> {
    diagnose(&a2::report()?)
}

fn main() -> R<()> {
    print!("{}", run()?.csv);
    Ok(())
}
