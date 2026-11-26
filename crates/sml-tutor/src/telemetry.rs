//! Counters and timings for the learning engine, with a dependency-free JSON-lines export.
//!
//! These are engine measurements: how many items were processed, how many
//! passed the examiner, how long each phase took. They say nothing about
//! whether any learner actually understands its domain.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::domain::Domain;

/// Event counts and phase durations for one scope (a domain, a machine, or the total).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub processed: u64,
    pub learned: u64,
    pub certified: u64,
    pub consolidated: u64,
    pub exam_failures: u64,
    pub abstentions: u64,
    pub teacher_proposals: u64,
    pub teacher_rejections: u64,
    pub attempts: u64,
    pub contradictions: u64,
    pub gaps_detected: u64,
    pub gaps_resolved: u64,
    pub learn_nanos: u128,
    pub validate_nanos: u128,
    pub consolidate_nanos: u128,
}

impl Counters {
    /// Adds every field of `other` into `self`, saturating on overflow.
    pub fn merge(&mut self, other: &Counters) {
        self.processed = self.processed.saturating_add(other.processed);
        self.learned = self.learned.saturating_add(other.learned);
        self.certified = self.certified.saturating_add(other.certified);
        self.consolidated = self.consolidated.saturating_add(other.consolidated);
        self.exam_failures = self.exam_failures.saturating_add(other.exam_failures);
        self.abstentions = self.abstentions.saturating_add(other.abstentions);
        self.teacher_proposals = self
            .teacher_proposals
            .saturating_add(other.teacher_proposals);
        self.teacher_rejections = self
            .teacher_rejections
            .saturating_add(other.teacher_rejections);
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.contradictions = self.contradictions.saturating_add(other.contradictions);
        self.gaps_detected = self.gaps_detected.saturating_add(other.gaps_detected);
        self.gaps_resolved = self.gaps_resolved.saturating_add(other.gaps_resolved);
        self.learn_nanos = self.learn_nanos.saturating_add(other.learn_nanos);
        self.validate_nanos = self.validate_nanos.saturating_add(other.validate_nanos);
        self.consolidate_nanos = self
            .consolidate_nanos
            .saturating_add(other.consolidate_nanos);
    }

    /// Every field as `(key, rendered JSON value)`, including the derived rates.
    fn json_fields(&self, wall_nanos: u128) -> Vec<(&'static str, String)> {
        vec![
            ("processed", self.processed.to_string()),
            ("learned", self.learned.to_string()),
            ("certified", self.certified.to_string()),
            ("consolidated", self.consolidated.to_string()),
            ("exam_failures", self.exam_failures.to_string()),
            ("abstentions", self.abstentions.to_string()),
            ("teacher_proposals", self.teacher_proposals.to_string()),
            ("teacher_rejections", self.teacher_rejections.to_string()),
            ("attempts", self.attempts.to_string()),
            ("contradictions", self.contradictions.to_string()),
            ("gaps_detected", self.gaps_detected.to_string()),
            ("gaps_resolved", self.gaps_resolved.to_string()),
            ("learn_nanos", self.learn_nanos.to_string()),
            ("validate_nanos", self.validate_nanos.to_string()),
            ("consolidate_nanos", self.consolidate_nanos.to_string()),
            (
                "certified_per_sec",
                format!("{:.6}", rate(self.certified, wall_nanos)),
            ),
            (
                "processed_per_sec",
                format!("{:.6}", rate(self.processed, wall_nanos)),
            ),
        ]
    }
}

/// Telemetry for one engine run: counters per domain and per machine, plus a start instant.
#[derive(Debug)]
pub struct Telemetry {
    by_domain: BTreeMap<Domain, Counters>,
    by_machine: BTreeMap<String, Counters>,
    started: Instant,
}

impl Telemetry {
    /// Creates empty telemetry whose clock starts now.
    pub fn new() -> Self {
        Telemetry {
            by_domain: BTreeMap::new(),
            by_machine: BTreeMap::new(),
            started: Instant::now(),
        }
    }

    /// Mutable counters for a domain, created on first use.
    pub fn domain_mut(&mut self, domain: Domain) -> &mut Counters {
        self.by_domain.entry(domain).or_default()
    }

    /// Mutable counters for a machine, created on first use.
    pub fn machine_mut(&mut self, machine: &str) -> &mut Counters {
        self.by_machine.entry(machine.to_owned()).or_default()
    }

    /// A copy of a domain's counters; all zeros if the domain was never touched.
    pub fn domain(&self, domain: Domain) -> Counters {
        self.by_domain.get(&domain).cloned().unwrap_or_default()
    }

    /// Sum of all domain counters. Machine counters are not added again.
    pub fn total(&self) -> Counters {
        let mut total = Counters::default();
        for c in self.by_domain.values() {
            total.merge(c);
        }
        total
    }

    /// Seconds since this telemetry was created.
    pub fn elapsed_secs(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Renders one JSON object per line: the four domains (always present, in
    /// `Domain::ALL` order), then each machine, then one total line. Rates are
    /// computed against `wall_nanos`. Each line starts with `kind`, then the
    /// identifying key, then the remaining keys in alphabetical order.
    pub fn to_json_lines(&self, wall_nanos: u128) -> String {
        let mut out = String::new();
        for domain in Domain::ALL {
            let fields = self.domain(domain).json_fields(wall_nanos);
            out.push_str(&render_line(
                "domain",
                Some(("domain", domain.name())),
                fields,
            ));
            out.push('\n');
        }
        for (name, counters) in &self.by_machine {
            let fields = counters.json_fields(wall_nanos);
            out.push_str(&render_line("machine", Some(("machine", name)), fields));
            out.push('\n');
        }
        let fields = self.total().json_fields(wall_nanos);
        out.push_str(&render_line("total", None, fields));
        out.push('\n');
        out
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Events per second over `nanos`; `0.0` when no time was recorded.
pub fn rate(count: u64, nanos: u128) -> f64 {
    if nanos == 0 {
        0.0
    } else {
        count as f64 / (nanos as f64 / 1e9)
    }
}

fn render_line(
    kind: &str,
    id: Option<(&str, &str)>,
    mut fields: Vec<(&'static str, String)>,
) -> String {
    fields.sort_by(|a, b| a.0.cmp(b.0));
    let mut line = String::from("{\"kind\":");
    push_json_string(&mut line, kind);
    if let Some((key, value)) = id {
        line.push(',');
        push_json_string(&mut line, key);
        line.push(':');
        push_json_string(&mut line, value);
    }
    for (key, value) in fields {
        line.push(',');
        push_json_string(&mut line, key);
        line.push(':');
        line.push_str(&value);
    }
    line.push('}');
    line
}

/// Appends `s` as a quoted JSON string. Escapes quotes, backslashes and all
/// control characters; other characters are emitted as UTF-8.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escaped(s: &str) -> String {
        let mut out = String::new();
        push_json_string(&mut out, s);
        out
    }

    #[test]
    fn json_escaper_handles_tricky_strings() {
        assert_eq!(escaped(""), "\"\"");
        assert_eq!(escaped("plain"), "\"plain\"");
        assert_eq!(escaped("a\"b"), "\"a\\\"b\"");
        assert_eq!(escaped("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(escaped("\\\""), "\"\\\\\\\"\"");
        assert_eq!(escaped("line\nfeed\r\ttab"), "\"line\\nfeed\\r\\ttab\"");
        assert_eq!(escaped("\u{0}\u{1}\u{1f}"), "\"\\u0000\\u0001\\u001f\"");
        assert_eq!(escaped("\u{08}\u{0c}"), "\"\\b\\f\"");
        assert_eq!(escaped("\u{7f}"), "\"\u{7f}\"");
        assert_eq!(escaped("naïve ✓"), "\"naïve ✓\"");
        assert_eq!(escaped("}{,:"), "\"}{,:\"");
    }

    #[test]
    fn four_domain_lines_are_always_present_in_order() {
        let t = Telemetry::new();
        let text = t.to_json_lines(1_000_000_000);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4 + 1);
        for (i, d) in Domain::ALL.iter().enumerate() {
            let expected_prefix = format!("{{\"kind\":\"domain\",\"domain\":\"{}\",", d.name());
            assert!(lines[i].starts_with(&expected_prefix), "{}", lines[i]);
            assert!(lines[i].ends_with('}'));
            assert!(lines[i].contains("\"processed\":0,"));
        }
        assert!(lines[4].starts_with("{\"kind\":\"total\","));
    }

    #[test]
    fn keys_after_identifier_are_sorted_and_complete() {
        let t = Telemetry::new();
        let text = t.to_json_lines(1);
        let first = text.lines().next().unwrap();
        // Every key is followed by `":`, so splitting on that yields the key
        // just before each separator; the final piece is the last value.
        let parts: Vec<&str> = first.split("\":").collect();
        let keys: Vec<&str> = parts[..parts.len() - 1]
            .iter()
            .filter_map(|part| part.rsplit('"').next())
            .collect();
        assert_eq!(&keys[..2], &["kind", "domain"]);
        let rest = &keys[2..];
        let mut sorted = rest.to_vec();
        sorted.sort_unstable();
        assert_eq!(rest, sorted.as_slice());
        assert_eq!(rest.len(), 17);
        assert!(rest.contains(&"certified_per_sec"));
        assert!(rest.contains(&"processed_per_sec"));
    }

    #[test]
    fn machine_names_are_escaped_and_lines_counted() {
        let mut t = Telemetry::new();
        t.machine_mut("box \"one\"\\\n").processed = 2;
        t.machine_mut("plain").processed = 5;
        let text = t.to_json_lines(0);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4 + 2 + 1);
        assert!(
            lines[4].starts_with("{\"kind\":\"machine\",\"machine\":\"box \\\"one\\\"\\\\\\n\",")
        );
        assert!(lines[5].starts_with("{\"kind\":\"machine\",\"machine\":\"plain\","));
        assert!(lines[6].starts_with("{\"kind\":\"total\","));
    }

    #[test]
    fn totals_equal_sum_of_domains() {
        let mut t = Telemetry::new();
        t.domain_mut(Domain::French).processed = 3;
        t.domain_mut(Domain::French).certified = 1;
        t.domain_mut(Domain::Rust).processed = 4;
        t.domain_mut(Domain::Rust).learn_nanos = 7;
        t.domain_mut(Domain::Mathematics).exam_failures = 9;
        t.domain_mut(Domain::English).abstentions = 2;
        let total = t.total();
        let mut summed = Counters::default();
        for d in Domain::ALL {
            summed.merge(&t.domain(d));
        }
        assert_eq!(total, summed);
        assert_eq!(total.processed, 7);
        assert_eq!(total.certified, 1);
        assert_eq!(total.learn_nanos, 7);
        assert_eq!(total.exam_failures, 9);
        assert_eq!(total.abstentions, 2);
        assert_eq!(t.domain(Domain::English).processed, 0);
    }

    #[test]
    fn json_lines_carry_values_and_rates() {
        let mut t = Telemetry::new();
        t.domain_mut(Domain::Rust).certified = 4;
        t.domain_mut(Domain::Rust).processed = 10;
        let text = t.to_json_lines(2_000_000_000);
        let rust = text
            .lines()
            .find(|l| l.contains("\"domain\":\"rust\""))
            .unwrap();
        assert!(rust.contains("\"certified\":4,"));
        assert!(rust.contains("\"certified_per_sec\":2.000000"));
        assert!(rust.contains("\"processed_per_sec\":5.000000"));
    }

    #[test]
    fn merge_adds_every_field() {
        let a = Counters {
            processed: 1,
            learned: 2,
            certified: 3,
            consolidated: 4,
            exam_failures: 5,
            abstentions: 6,
            teacher_proposals: 7,
            teacher_rejections: 8,
            attempts: 9,
            contradictions: 10,
            gaps_detected: 11,
            gaps_resolved: 12,
            learn_nanos: 13,
            validate_nanos: 14,
            consolidate_nanos: 15,
        };
        let mut sum = a.clone();
        sum.merge(&a);
        assert_eq!(sum.processed, 2);
        assert_eq!(sum.gaps_resolved, 24);
        assert_eq!(sum.consolidate_nanos, 30);
        assert_eq!(sum.teacher_rejections, 16);

        let mut z = Counters::default();
        z.merge(&a);
        assert_eq!(z, a);

        let mut big = Counters {
            processed: u64::MAX,
            learn_nanos: u128::MAX,
            ..Counters::default()
        };
        big.merge(&a);
        assert_eq!(big.processed, u64::MAX);
        assert_eq!(big.learn_nanos, u128::MAX);
    }

    #[test]
    fn rate_edge_cases() {
        assert_eq!(rate(10, 0), 0.0);
        assert_eq!(rate(0, 1), 0.0);
        assert_eq!(rate(10, 2_000_000_000), 5.0);
        assert_eq!(rate(1, 1_000_000_000), 1.0);
        assert!(rate(u64::MAX, u128::MAX).is_finite());
    }

    #[test]
    fn elapsed_is_non_negative() {
        let t = Telemetry::default();
        assert!(t.elapsed_secs() >= 0.0);
    }
}
