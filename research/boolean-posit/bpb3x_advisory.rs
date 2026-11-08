//! BPB3-X pure, non-actuating advisory selection research gate.
//! Run independently: rustc --edition=2021 --test research/boolean-posit/bpb3x_advisory.rs -o /tmp/bpb3x-advisory-tests && /tmp/bpb3x-advisory-tests
//! This is NOT ElasticXxx facade integration or physical measurement.
#![forbid(unsafe_code)]
use std::collections::BTreeMap;

pub const LAYOUTS: [&[u16]; 6] = [
    &[256], &[128, 128], &[128, 64, 64],
    &[64, 128, 64], &[64, 64, 128], &[64, 64, 64, 64],
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub layout_id: u8,
    pub source_sha: String,
    pub hardware_id: String,
    pub workload_id: String,
    pub measured_at_ms: u64,
    pub latency_ns: u64,
    pub oracle_passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub source_sha: &'a str,
    pub hardware_id: &'a str,
    pub workload_id: &'a str,
    pub now_ms: u64,
    pub max_age_ms: u64,
    pub fixed_baseline_id: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    InvalidRequest,
    InvalidLayout,
    IdentityMismatch,
    StaleOrFuture,
    OracleFailed,
    DuplicateLayout,
    MissingCandidate,
    BaselineUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advice {
    pub selected_id: u8,
    pub fixed_baseline_id: u8,
    pub selected_latency_ns: u64,
    pub baseline_latency_ns: u64,
    // This result is diagnostic only; never an authorization to mutate weights.
}

pub fn advise(request: &Request<'_>, observations: &[Observation]) -> Result<Advice, Reject> {
    if request.source_sha.is_empty() || request.hardware_id.is_empty()
        || request.workload_id.is_empty() || request.max_age_ms == 0
        || usize::from(request.fixed_baseline_id) >= LAYOUTS.len()
    {
        return Err(Reject::InvalidRequest);
    }
    let mut candidates = BTreeMap::new();
    for observation in observations {
        if usize::from(observation.layout_id) >= LAYOUTS.len() {
            return Err(Reject::InvalidLayout);
        }
        if observation.source_sha != request.source_sha
            || observation.hardware_id != request.hardware_id
            || observation.workload_id != request.workload_id
        {
            return Err(Reject::IdentityMismatch);
        }
        let age = request.now_ms.checked_sub(observation.measured_at_ms)
            .ok_or(Reject::StaleOrFuture)?;
        if age > request.max_age_ms {
            return Err(Reject::StaleOrFuture);
        }
        if !observation.oracle_passed || observation.latency_ns == 0 {
            return Err(Reject::OracleFailed);
        }
        if candidates.insert(observation.layout_id, observation.latency_ns).is_some() {
            return Err(Reject::DuplicateLayout);
        }
    }
    // Require all six prequalified layouts, rather than silently selecting
    // among a biased subset of observations.
    if candidates.len() != LAYOUTS.len() {
        return Err(Reject::MissingCandidate);
    }
    let baseline_latency_ns = *candidates.get(&request.fixed_baseline_id)
        .ok_or(Reject::BaselineUnavailable)?;
    let (&selected_id, &selected_latency_ns) = candidates.iter()
        .min_by_key(|(id, latency)| (**latency, **id))
        .ok_or(Reject::MissingCandidate)?;
    Ok(Advice {
        selected_id,
        fixed_baseline_id: request.fixed_baseline_id,
        selected_latency_ns,
        baseline_latency_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request<'a>() -> Request<'a> {
        Request { source_sha: "pinned", hardware_id: "host", workload_id: "partial64",
            now_ms: 1000, max_age_ms: 100, fixed_baseline_id: 0 }
    }
    fn observations() -> Vec<Observation> {
        (0..6).map(|id| Observation {
            layout_id: id, source_sha: "pinned".into(), hardware_id: "host".into(),
            workload_id: "partial64".into(), measured_at_ms: 950,
            latency_ns: if id == 3 { 10 } else { 20 }, oracle_passed: true,
        }).collect()
    }
    #[test]
    fn exact_partition_catalogue() {
        assert_eq!(LAYOUTS.len(), 6);
        for layout in LAYOUTS {
            assert_eq!(layout.iter().sum::<u16>(), 256);
            assert!(layout.iter().all(|x| [64, 128, 256].contains(x)));
        }
    }
    #[test]
    fn deterministic_and_non_actuating() {
        let input = observations();
        let snapshot = input.clone();
        let advice = advise(&request(), &input).unwrap();
        assert_eq!(advice.selected_id, 3);
        assert_eq!(advice.baseline_latency_ns, 20);
        assert_eq!(input, snapshot);
    }
    #[test]
    fn reject_missing_and_duplicate() {
        let mut input = observations();
        input.pop();
        assert_eq!(advise(&request(), &input), Err(Reject::MissingCandidate));
        input = observations();
        input.push(input[0].clone());
        assert_eq!(advise(&request(), &input), Err(Reject::DuplicateLayout));
    }
    #[test]
    fn reject_stale_future_and_wrong_identity() {
        let mut input = observations();
        input[0].measured_at_ms = 899;
        assert_eq!(advise(&request(), &input), Err(Reject::StaleOrFuture));
        input = observations();
        input[0].measured_at_ms = 1001;
        assert_eq!(advise(&request(), &input), Err(Reject::StaleOrFuture));
        input = observations();
        input[0].source_sha = "other".into();
        assert_eq!(advise(&request(), &input), Err(Reject::IdentityMismatch));
    }
    #[test]
    fn reject_failed_oracle_and_invalid_layout() {
        let mut input = observations();
        input[0].oracle_passed = false;
        assert_eq!(advise(&request(), &input), Err(Reject::OracleFailed));
        input = observations();
        input[0].layout_id = 6;
        assert_eq!(advise(&request(), &input), Err(Reject::InvalidLayout));
    }
}
