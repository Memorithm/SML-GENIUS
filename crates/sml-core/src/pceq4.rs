//! PCEQ-4: version-bound producer snapshots for research-only sparse quantization.
//!
//! A certified zero lets the quantizer skip *all* source reads only when its
//! assertion is truthful. Tracking producer identity and monotonic revisions
//! can detect some stale metadata; it CANNOT prove a zero assertion is true.
//! No shared-memory concurrency / ABA safety is implied by this trait.

use crate::pceq0::{PceqStreamingError, ProducerBound};
use crate::pceq3::{quantize_sparse_bounded, SparsePceqI16};
use std::cell::Cell;

/// Upstream producer owns both samples and their precomputed envelopes.
///
/// Implementors MUST ensure `revision()` is monotonic and changes whenever
/// either data or certificate changes. This trait is a scientific contract,
/// not an enforcement mechanism. For concurrent mutation, implementors must
/// arrange a coherent immutable published version, not rely on this check.
pub trait VersionedBlockProducer {
    fn producer_id(&self) -> u128;
    fn revision(&self) -> u64;
    fn block_bound(&self, block: usize) -> ProducerBound;
    fn sample(&self, index: usize) -> i16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionLocation {
    BeforeBlock(usize),
    AfterBound(usize),
    BeforeSample(usize),
    AfterSample(usize),
    BeforePublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pceq4Error {
    Quantizer(PceqStreamingError),
    RevisionChanged {
        expected: u64,
        observed: u64,
        location: RevisionLocation,
    },
    IdentityChanged {
        expected: u128,
        observed: u128,
    },
}

/// Published only after the full sparse encoding succeeds and both publisher
/// identity and revision pass a final validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedSparsePceqI16 {
    producer_id: u128,
    revision: u64,
    snapshot: SparsePceqI16,
}

impl VersionedSparsePceqI16 {
    #[must_use]
    pub const fn producer_id(&self) -> u128 {
        self.producer_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn snapshot(&self) -> &SparsePceqI16 {
        &self.snapshot
    }
}

fn verify_revision<P: VersionedBlockProducer>(
    source: &P,
    expected: u64,
    location: RevisionLocation,
    first_error: &Cell<Option<Pceq4Error>>,
) -> bool {
    if first_error.get().is_some() {
        return false;
    }
    let observed = source.revision();
    if observed != expected {
        first_error.set(Some(Pceq4Error::RevisionChanged {
            expected,
            observed,
            location,
        }));
        false
    } else {
        true
    }
}

/// A source-bound snapshot with no unquantized block rereads and no published
/// inactive i8 payload. Metadata revision checks are added before/after each
/// block bound and sample; this costs extra metadata loads, not source rereads.
///
/// On a changed revision, callbacks return placeholder zeros/certificates
/// and the entire result is discarded after the underlying algorithm returns.
/// No incomplete snapshot can escape. This is not a zero-proof verifier.
///
/// The version must be monotonic, and no producer may ever mutate any source
/// block or certificate without incrementing the version. Otherwise changes
/// (including ABA) remain undetectable.
pub fn quantize_sparse_versioned<P: VersionedBlockProducer>(
    len: usize,
    block_size: usize,
    source: &P,
) -> Result<VersionedSparsePceqI16, Pceq4Error> {
    let producer_id = source.producer_id();
    let revision = source.revision();
    let first_error: Cell<Option<Pceq4Error>> = Cell::new(None);
    let encoded = quantize_sparse_bounded(
        len,
        block_size,
        |index| {
            if !verify_revision(
                source,
                revision,
                RevisionLocation::BeforeSample(index),
                &first_error,
            ) {
                return 0;
            }
            let value = source.sample(index);
            if verify_revision(
                source,
                revision,
                RevisionLocation::AfterSample(index),
                &first_error,
            ) {
                value
            } else {
                0
            }
        },
        |block| {
            if !verify_revision(
                source,
                revision,
                RevisionLocation::BeforeBlock(block),
                &first_error,
            ) {
                return ProducerBound::CertifiedZero;
            }
            let bound = source.block_bound(block);
            if verify_revision(
                source,
                revision,
                RevisionLocation::AfterBound(block),
                &first_error,
            ) {
                bound
            } else {
                ProducerBound::CertifiedZero
            }
        },
    );

    // A source can change even on a certified-zero final block, which has no
    // sample callback; every successful result is checked before publication.
    let final_identity = source.producer_id();
    if final_identity != producer_id {
        return Err(Pceq4Error::IdentityChanged {
            expected: producer_id,
            observed: final_identity,
        });
    }
    if let Some(error) = first_error.get() {
        return Err(error);
    }
    let end_revision = source.revision();
    if end_revision != revision {
        return Err(Pceq4Error::RevisionChanged {
            expected: revision,
            observed: end_revision,
            location: RevisionLocation::BeforePublication,
        });
    }

    Ok(VersionedSparsePceqI16 {
        producer_id,
        revision,
        snapshot: encoded.map_err(Pceq4Error::Quantizer)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        id: Cell<u128>,
        revision: Cell<u64>,
        input: Vec<i16>,
        bounds: Vec<ProducerBound>,
        changed_sample: Option<usize>,
        changed_bound: Option<usize>,
        samples: Cell<usize>,
        bounds_read: Cell<usize>,
    }

    impl VersionedBlockProducer for Fixture {
        fn producer_id(&self) -> u128 {
            self.id.get()
        }

        fn revision(&self) -> u64 {
            self.revision.get()
        }

        fn block_bound(&self, block: usize) -> ProducerBound {
            self.bounds_read.set(self.bounds_read.get() + 1);
            if self.changed_bound == Some(block) {
                self.revision.set(8);
            }
            self.bounds[block]
        }

        fn sample(&self, index: usize) -> i16 {
            self.samples.set(self.samples.get() + 1);
            if self.changed_sample == Some(index) {
                self.revision.set(8);
            }
            self.input[index]
        }
    }

    fn fixture(len: usize, block_size: usize) -> Fixture {
        let mut input = vec![0_i16; len];
        let mut bounds = Vec::new();
        for start in (0..len).step_by(block_size) {
            let block = start / block_size;
            if block % 2 == 1 {
                input[start] = i16::MIN;
                if start + 1 < len {
                    input[start + 1] = i16::MAX;
                }
                bounds.push(ProducerBound::MaxAbs(32_768));
            } else {
                bounds.push(ProducerBound::CertifiedZero);
            }
        }
        Fixture {
            id: Cell::new(0xabc123),
            revision: Cell::new(7),
            input,
            bounds,
            changed_sample: None,
            changed_bound: None,
            samples: Cell::new(0),
            bounds_read: Cell::new(0),
        }
    }

    #[test]
    fn stable_version_parity_with_sparse_reference_and_zero_skips() {
        for len in [0, 1, 32, 33, 63, 64, 65, 128, 129, 256, 4097] {
            for block in [32, 64, 128] {
                let source = fixture(len, block);
                let tagged = quantize_sparse_versioned(len, block, &source).unwrap();
                let reference =
                    quantize_sparse_bounded(len, block, |i| source.input[i], |b| source.bounds[b])
                        .unwrap();
                assert_eq!(tagged.snapshot(), &reference);
                assert_eq!(tagged.producer_id(), 0xabc123);
                assert_eq!(tagged.revision(), 7);
                assert_eq!(source.bounds_read.get(), len.div_ceil(block));
                assert_eq!(
                    source.samples.get(),
                    (0..len).filter(|&i| i / block % 2 == 1).count()
                );
                assert_eq!(tagged.snapshot().reconstruct(), reference.reconstruct());
            }
        }
    }

    #[test]
    fn changed_revision_while_sampling_fails_without_snapshot() {
        let mut source = fixture(128, 32);
        source.changed_sample = Some(33);
        assert_eq!(
            quantize_sparse_versioned(128, 32, &source),
            Err(Pceq4Error::RevisionChanged {
                expected: 7,
                observed: 8,
                location: RevisionLocation::AfterSample(33),
            })
        );
        assert_eq!(source.samples.get(), 2);
    }

    #[test]
    fn changed_revision_while_generating_bound_fails() {
        let mut source = fixture(128, 32);
        source.changed_bound = Some(2);
        assert_eq!(
            quantize_sparse_versioned(128, 32, &source),
            Err(Pceq4Error::RevisionChanged {
                expected: 7,
                observed: 8,
                location: RevisionLocation::AfterBound(2),
            })
        );
        assert_eq!(source.bounds_read.get(), 3);
        assert_eq!(source.samples.get(), 32);
    }

    #[test]
    fn invalid_bound_and_invalid_block_size_remain_fail_closed() {
        let mut source = fixture(32, 32);
        source.bounds[0] = ProducerBound::MaxAbs(32_769);
        assert_eq!(
            quantize_sparse_versioned(32, 32, &source),
            Err(Pceq4Error::Quantizer(PceqStreamingError::OutOfRangeBound {
                block: 0,
                bound: 32_769,
            }))
        );
        assert_eq!(source.samples.get(), 0);
        assert_eq!(
            quantize_sparse_versioned(32, 0, &source),
            Err(Pceq4Error::Quantizer(PceqStreamingError::InvalidBlockSize(
                0
            )))
        );
    }

    #[test]
    fn false_zero_certification_is_not_a_proof() {
        let mut source = fixture(32, 32);
        source.input[0] = 99;
        // Deliberately malicious/inconsistent assertion while keeping epoch.
        // Revision metadata cannot detect a false zero block if it never reads
        // the samples; this tests and documents that security limitation.
        let result = quantize_sparse_versioned(32, 32, &source).unwrap();
        assert_eq!(source.samples.get(), 0);
        assert_eq!(result.snapshot().reconstruct(), vec![0; 32]);
    }
}
