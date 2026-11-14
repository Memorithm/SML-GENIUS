//! Canonical page directory and exact bipolar matvec over original BPB-3-A8 wires.
//! A research-only CPU tensor reader; not a full LLM or native Q3_K replacement.
#![forbid(unsafe_code)]

#[path = "bpb3_a8_superblock.rs"]
#[allow(dead_code)]
mod a8;
#[path = "bpb3_a8_paged.rs"]
#[allow(dead_code)]
mod paged;

const MAGIC: [u8; 4] = *b"BA8T";
const HEADER_BYTES: usize = 12;
const MAX_PAGES: usize = 4096;
const ROWS_PER_PAGE: usize = paged::MAX_BLOCKS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorError {
    Empty,
    TooManyPages,
    InvalidHeader,
    InvalidDirectory,
    InvalidPage,
    WrongLength,
    OutOfRange,
    Overflow,
}

impl From<paged::PageError> for TensorError {
    fn from(_: paged::PageError) -> Self {
        Self::InvalidPage
    }
}

/// Bytes charged for the entire tensor-level directory, *not* including
/// page-local checkpoint/index overhead or the original weight wires.
pub fn directory_bytes(row_count: usize) -> Result<usize, TensorError> {
    if row_count == 0 {
        return Err(TensorError::Empty);
    }
    let pages = row_count.div_ceil(ROWS_PER_PAGE);
    if pages > MAX_PAGES {
        return Err(TensorError::TooManyPages);
    }
    HEADER_BYTES
        .checked_add(
            pages
                .checked_add(1)
                .ok_or(TensorError::Overflow)?
                .checked_mul(4)
                .ok_or(TensorError::Overflow)?,
        )
        .ok_or(TensorError::Overflow)
}

/// Entire serialized footprint including A8 payloads, page checkpoints and
/// the tensor-level directory. Never present raw A8 payload as all-in cost.
pub fn encode_tensor(blocks: &[Vec<u8>]) -> Result<Vec<u8>, TensorError> {
    let directory_len = directory_bytes(blocks.len())?;
    let count = blocks.len().div_ceil(ROWS_PER_PAGE);
    let mut pages = Vec::with_capacity(count);
    let mut positions = Vec::with_capacity(count + 1);
    let mut payload_size = 0usize;
    positions.push(0u32);
    for chunk in blocks.chunks(ROWS_PER_PAGE) {
        let page = paged::encode_page(chunk)?;
        payload_size = payload_size
            .checked_add(page.len())
            .ok_or(TensorError::Overflow)?;
        positions.push(u32::try_from(payload_size).map_err(|_| TensorError::Overflow)?);
        pages.push(page);
    }
    let row_count = u32::try_from(blocks.len()).map_err(|_| TensorError::Overflow)?;
    let page_count = u32::try_from(count).map_err(|_| TensorError::Overflow)?;
    let mut out = Vec::with_capacity(
        directory_len
            .checked_add(payload_size)
            .ok_or(TensorError::Overflow)?,
    );
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&row_count.to_le_bytes());
    out.extend_from_slice(&page_count.to_le_bytes());
    for offset in positions {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    for page in pages {
        out.extend_from_slice(&page);
    }
    Ok(out)
}

/// Once validated, all page references are immutable borrows of source bytes.
/// A tensor row lookup takes O(1) page-directory access plus <=7 preceding
/// local length prefixes; it does not revalidate the entire page.
pub struct TensorView<'a> {
    pages: Vec<paged::ValidatedPage<'a>>,
    rows: usize,
    wire_bytes: usize,
}

impl<'a> TensorView<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, TensorError> {
        let header = bytes.get(..HEADER_BYTES).ok_or(TensorError::WrongLength)?;
        if header[..4] != MAGIC {
            return Err(TensorError::InvalidHeader);
        }
        let rows = usize::try_from(u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| TensorError::InvalidHeader)?,
        ))
        .map_err(|_| TensorError::Overflow)?;
        let pages = usize::try_from(u32::from_le_bytes(
            header[8..12]
                .try_into()
                .map_err(|_| TensorError::InvalidHeader)?,
        ))
        .map_err(|_| TensorError::Overflow)?;
        let header_len = directory_bytes(rows)?;
        if pages != rows.div_ceil(ROWS_PER_PAGE) || bytes.len() < header_len {
            return Err(TensorError::InvalidDirectory);
        }
        let payload = &bytes[header_len..];
        let positions: Vec<usize> = (0..=pages)
            .map(|index| {
                let at = HEADER_BYTES + index * 4;
                usize::try_from(u32::from_le_bytes(
                    bytes[at..at + 4]
                        .try_into()
                        .map_err(|_| TensorError::InvalidDirectory)?,
                ))
                .map_err(|_| TensorError::Overflow)
            })
            .collect::<Result<_, _>>()?;
        if positions.first() != Some(&0) || positions.last() != Some(&payload.len()) {
            return Err(TensorError::InvalidDirectory);
        }
        let mut decoded = Vec::with_capacity(pages);
        for (page_no, pair) in positions.windows(2).enumerate() {
            if pair[0] >= pair[1] || pair[1] > payload.len() {
                return Err(TensorError::InvalidDirectory);
            }
            let page = paged::ValidatedPage::new(&payload[pair[0]..pair[1]])?;
            let expected_rows = if page_no + 1 == pages {
                rows - page_no * ROWS_PER_PAGE
            } else {
                ROWS_PER_PAGE
            };
            if page.len() != expected_rows {
                return Err(TensorError::InvalidDirectory);
            }
            decoded.push(page);
        }
        Ok(Self {
            pages: decoded,
            rows,
            wire_bytes: bytes.len(),
        })
    }

    pub fn len(&self) -> usize {
        self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    pub fn total_serialized_bytes(&self) -> usize {
        self.wire_bytes
    }

    pub fn row(&self, index: usize) -> Result<&'a [u8], TensorError> {
        if index >= self.rows {
            return Err(TensorError::OutOfRange);
        }
        self.pages[index / ROWS_PER_PAGE]
            .get(index % ROWS_PER_PAGE)
            .map_err(Into::into)
    }

    /// Each output uses the same 256-element bipolar vector; a set input bit
    /// denotes -1, otherwise +1. Exact output units are multiples of 2^-24.
    /// No dense dequantization occurs in this execution path.
    /// Reuses caller-owned output storage across multiple inference inputs.
    /// Wrong-sized output buffers are rejected before modifying any element.
    pub fn signed_bipolar_matvec_into(
        &self,
        mask: &[u64; 4],
        outputs: &mut [i128],
    ) -> Result<(), TensorError> {
        if outputs.len() != self.rows {
            return Err(TensorError::WrongLength);
        }
        for (row, output) in outputs.iter_mut().enumerate() {
            *output = a8::dot_exact(self.row(row)?, mask).map_err(|_| TensorError::InvalidPage)?;
        }
        Ok(())
    }

    /// Exact signed INT8 matvec, in units of 2^-24, with one canonical
    /// A8 parse per row. Eight activation planes are prepared once and
    /// reused across every tensor row; storage remains original A8 wire.
    /// No model-level accuracy or throughput claim follows from this path.
    pub fn signed_i8_matvec_into(
        &self,
        activations: &[i8; 256],
        outputs: &mut [i128],
    ) -> Result<(), TensorError> {
        if outputs.len() != self.rows {
            return Err(TensorError::WrongLength);
        }
        let masks: [[u16; 16]; 8] = std::array::from_fn(|bit| {
            std::array::from_fn(|group| {
                let mut mask = 0u16;
                for offset in 0..16 {
                    let code = activations[group * 16 + offset] as u8;
                    mask |= u16::from((code >> bit) & 1) << offset;
                }
                mask
            })
        });
        for (row, output) in outputs.iter_mut().enumerate() {
            *output = a8::dot_i8_fused(self.row(row)?, &masks).map_err(|error| match error {
                a8::Error::Overflow => TensorError::Overflow,
                _ => TensorError::InvalidPage,
            })?;
        }
        Ok(())
    }

    /// Four distinct signed INT8 vectors against the same stored tensor.
    /// Prepares all activation masks once and parses each A8 row once
    /// for four independent outputs, in row-major [row][lane] order.
    /// Incorrect output cardinality is rejected without modification.
    /// Evaluate an arbitrary compile-time batch of signed INT8 vectors over
    /// each canonical A8 row using a single weight parse per row.
    /// Existing batch4 calls are retained as stable wrappers.
    pub fn signed_i8_matvec_batch_into<const LANES: usize>(
        &self,
        inputs: &[[i8; 256]; LANES],
        outputs: &mut [[i128; LANES]],
    ) -> Result<(), TensorError> {
        if outputs.len() != self.rows {
            return Err(TensorError::WrongLength);
        }
        let masks: [[[u16; 16]; 8]; LANES] = std::array::from_fn(|lane| {
            std::array::from_fn(|bit| {
                std::array::from_fn(|group| {
                    let mut mask = 0u16;
                    for offset in 0..16 {
                        let value = inputs[lane][group * 16 + offset] as u8;
                        mask |= u16::from((value >> bit) & 1) << offset;
                    }
                    mask
                })
            })
        });
        for (row, output) in outputs.iter_mut().enumerate() {
            *output =
                a8::dot_i8_fused_batch(self.row(row)?, &masks).map_err(|error| match error {
                    a8::Error::Overflow => TensorError::Overflow,
                    _ => TensorError::InvalidPage,
                })?;
        }
        Ok(())
    }

    pub fn signed_i8_matvec_batch4_into(
        &self,
        inputs: &[[i8; 256]; 4],
        outputs: &mut [[i128; 4]],
    ) -> Result<(), TensorError> {
        self.signed_i8_matvec_batch_into(inputs, outputs)
    }

    pub fn signed_i8_matvec_batch8_into(
        &self,
        inputs: &[[i8; 256]; 8],
        outputs: &mut [[i128; 8]],
    ) -> Result<(), TensorError> {
        self.signed_i8_matvec_batch_into(inputs, outputs)
    }

    pub fn signed_i8_matvec_batch8(
        &self,
        inputs: &[[i8; 256]; 8],
    ) -> Result<Vec<[i128; 8]>, TensorError> {
        let mut outputs = vec![[0i128; 8]; self.rows];
        self.signed_i8_matvec_batch8_into(inputs, &mut outputs)?;
        Ok(outputs)
    }

    pub fn signed_i8_matvec_batch4(
        &self,
        inputs: &[[i8; 256]; 4],
    ) -> Result<Vec<[i128; 4]>, TensorError> {
        let mut outputs = vec![[0i128; 4]; self.rows];
        self.signed_i8_matvec_batch4_into(inputs, &mut outputs)?;
        Ok(outputs)
    }

    pub fn signed_i8_matvec(&self, activations: &[i8; 256]) -> Result<Vec<i128>, TensorError> {
        let mut outputs = vec![0; self.rows];
        self.signed_i8_matvec_into(activations, &mut outputs)?;
        Ok(outputs)
    }

    pub fn signed_bipolar_matvec(&self, mask: &[u64; 4]) -> Result<Vec<i128>, TensorError> {
        let mut outputs = vec![0i128; self.rows];
        self.signed_bipolar_matvec_into(mask, &mut outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(index: usize) -> Vec<u8> {
        let base = (index % 50) as i8 - 23;
        let groups = std::array::from_fn(|g| {
            let mode = 1 + ((index + g) % 3) as u8;
            let mut codes = [0i8; 16];
            codes[(index + g) % 16] = if (index + g) % 2 == 0 { 1 } else { -1 };
            a8::Group {
                mode,
                exponent: base,
                codes,
                sse: 0.0,
            }
        });
        a8::pack(&a8::Superblock {
            base,
            groups,
            sse: 0.0,
        })
        .unwrap()
    }

    fn scalar_oracle(row: &[u8], mask: &[u64; 4]) -> i128 {
        let (weights, exponents) = a8::unpack(row).unwrap();
        weights.iter().enumerate().fold(0i128, |sum, (i, &weight)| {
            let bipolar = if (mask[i / 64] >> (i % 64)) & 1 == 0 {
                1i128
            } else {
                -1i128
            };
            sum + i128::from(weight) * bipolar * (1i128 << (i32::from(exponents[i / 16]) + 24))
        })
    }

    #[test]
    fn every_page_boundary_preserves_exact_bytes() {
        for count in [1, 24, 255, 256, 257, 511, 512, 513] {
            let source: Vec<Vec<u8>> = (0..count).map(make_row).collect();
            let encoded = encode_tensor(&source).unwrap();
            let view = TensorView::new(&encoded).unwrap();
            assert_eq!(view.len(), count);
            assert_eq!(view.total_serialized_bytes(), encoded.len());
            for (index, expected) in source.iter().enumerate() {
                assert_eq!(view.row(index).unwrap(), expected);
            }
            assert_eq!(view.row(count), Err(TensorError::OutOfRange));
            let payload: usize = source.iter().map(Vec::len).sum();
            let pages = count.div_ceil(ROWS_PER_PAGE);
            let page_overhead: usize = (0..pages)
                .map(|p| {
                    let page_rows = (count - p * ROWS_PER_PAGE).min(ROWS_PER_PAGE);
                    paged::page_index_bytes(page_rows).unwrap()
                })
                .sum();
            assert_eq!(
                encoded.len(),
                payload + page_overhead + directory_bytes(count).unwrap()
            );
        }
    }

    #[test]
    fn integer_bipolar_matvec_matches_dense_oracle_across_pages() {
        let source: Vec<Vec<u8>> = (0..300).map(make_row).collect();
        let encoded = encode_tensor(&source).unwrap();
        let view = TensorView::new(&encoded).unwrap();
        for mask in [
            [0u64; 4],
            [u64::MAX; 4],
            [0xaaaa_aaaa_aaaa_aaaa; 4],
            [0x0123_4567_89ab_cdef; 4],
        ] {
            let actual = view.signed_bipolar_matvec(&mask).unwrap();
            let reference: Vec<_> = source.iter().map(|row| scalar_oracle(row, &mask)).collect();
            assert_eq!(actual, reference);
            let mut reused = vec![-12345i128; view.len()];
            view.signed_bipolar_matvec_into(&mask, &mut reused).unwrap();
            assert_eq!(reused, reference);
            let mut too_short = vec![77i128; view.len() - 1];
            assert_eq!(
                view.signed_bipolar_matvec_into(&mask, &mut too_short),
                Err(TensorError::WrongLength)
            );
            assert!(too_short.iter().all(|&value| value == 77));
        }
    }

    #[test]
    fn signed_i8_tensor_matvec_matches_independent_scalar_across_pages() {
        let rows: Vec<Vec<u8>> = (0..257).map(make_row).collect();
        let encoded = encode_tensor(&rows).unwrap();
        let view = TensorView::new(&encoded).unwrap();
        let samples: [[i8; 256]; 5] = [
            [0; 256],
            [-128; 256],
            [127; 256],
            std::array::from_fn(|i| if i % 2 == 0 { -128 } else { 127 }),
            std::array::from_fn(|i| (i as u8).wrapping_mul(37) as i8),
        ];
        for activations in samples {
            let expected: Vec<i128> = rows
                .iter()
                .map(|wire| {
                    let (weights, exponents) = a8::unpack(wire).unwrap();
                    (0..256)
                        .map(|i| {
                            i128::from(weights[i])
                                * i128::from(activations[i])
                                * (1i128 << (i32::from(exponents[i / 16]) + 24))
                        })
                        .sum()
                })
                .collect();
            assert_eq!(view.signed_i8_matvec(&activations).unwrap(), expected);
            let mut reuse = vec![0i128; view.len()];
            view.signed_i8_matvec_into(&activations, &mut reuse)
                .unwrap();
            assert_eq!(reuse, expected);
            let mut wrong = vec![99i128; view.len() - 1];
            assert_eq!(
                view.signed_i8_matvec_into(&activations, &mut wrong),
                Err(TensorError::WrongLength)
            );
            assert!(wrong.iter().all(|&value| value == 99));
        }
    }

    #[test]
    fn dense_signed_i8_all_weight_planes_and_scale_boundaries() {
        // The earlier sparse fixture only has one nonzero coefficient per
        // group. Exercise all 256 coefficients, all magnitude planes and
        // both exponent boundaries against an independently decoded oracle.
        let source: Vec<Vec<u8>> = (0..33)
            .map(|row| {
                let base = if row % 2 == 0 { -24 } else { 27 };
                let groups = std::array::from_fn(|group| {
                    let mode = 1 + ((row + group) % 3) as u8;
                    let max = (1i8 << mode) - 1;
                    let codes = std::array::from_fn(|offset| {
                        let value = ((row * 7 + group * 11 + offset * 13) % 7) as i8;
                        let magnitude = value % (max + 1);
                        if (row + group + offset) % 2 == 0 {
                            magnitude
                        } else {
                            -magnitude
                        }
                    });
                    a8::Group {
                        mode,
                        exponent: base,
                        codes,
                        sse: 0.0,
                    }
                });
                a8::pack(&a8::Superblock {
                    base,
                    groups,
                    sse: 0.0,
                })
                .unwrap()
            })
            .collect();
        let bytes = encode_tensor(&source).unwrap();
        let view = TensorView::new(&bytes).unwrap();
        let activations: [[i8; 256]; 4] = [
            [-128; 256],
            [127; 256],
            std::array::from_fn(|i| i as u8 as i8),
            std::array::from_fn(|i| {
                if i % 3 == 0 {
                    -1
                } else if i % 3 == 1 {
                    1
                } else {
                    -128
                }
            }),
        ];
        for input in activations {
            let outputs = view.signed_i8_matvec(&input).unwrap();
            for (index, row) in source.iter().enumerate() {
                let (codes, exponents) = a8::unpack(row).unwrap();
                let expected: i128 = (0..256)
                    .map(|i| {
                        i128::from(codes[i])
                            * i128::from(input[i])
                            * (1i128 << (i32::from(exponents[i / 16]) + 24))
                    })
                    .sum();
                assert_eq!(outputs[index], expected, "row {index}");
            }
        }
    }

    #[test]
    fn four_lane_int8_is_exact_against_independent_scalar_and_single_lane() {
        let rows: Vec<Vec<u8>> = (0..257).map(make_row).collect();
        let bytes = encode_tensor(&rows).unwrap();
        let view = TensorView::new(&bytes).unwrap();
        let inputs: [[i8; 256]; 4] = [
            [-128; 256],
            [127; 256],
            std::array::from_fn(|i| i as u8 as i8),
            std::array::from_fn(|i| (i as u8).wrapping_mul(53) as i8),
        ];
        let results = view.signed_i8_matvec_batch4(&inputs).unwrap();
        for lane in 0..4 {
            let single = view.signed_i8_matvec(&inputs[lane]).unwrap();
            for (row, wire) in rows.iter().enumerate() {
                let (weights, exponents) = a8::unpack(wire).unwrap();
                let scalar: i128 = (0..256)
                    .map(|i| {
                        i128::from(weights[i])
                            * i128::from(inputs[lane][i])
                            * (1i128 << (i32::from(exponents[i / 16]) + 24))
                    })
                    .sum();
                assert_eq!(results[row][lane], single[row]);
                assert_eq!(results[row][lane], scalar);
            }
        }
        let mut reused = vec![[0i128; 4]; view.len()];
        view.signed_i8_matvec_batch4_into(&inputs, &mut reused)
            .unwrap();
        assert_eq!(reused, results);
        let mut short = vec![[91i128; 4]; view.len() - 1];
        assert_eq!(
            view.signed_i8_matvec_batch4_into(&inputs, &mut short),
            Err(TensorError::WrongLength)
        );
        assert!(short.iter().all(|row| row.iter().all(|&v| v == 91)));
    }

    #[test]
    fn batch4_compact_accumulation_matches_dense_extreme_exponents() {
        // Exercise all magnitude planes, zero/sign patterns and both legal
        // exponent boundaries; this stresses the documented i32 bound.
        let rows: Vec<Vec<u8>> = (0..33)
            .map(|row| {
                let base = if row % 2 == 0 { -24 } else { 27 };
                let groups = std::array::from_fn(|group| {
                    let mode = 1 + ((row + group) % 3) as u8;
                    let max = (1i8 << mode) - 1;
                    let codes = std::array::from_fn(|offset| {
                        let magnitude = if offset % 3 == 0 {
                            max
                        } else {
                            ((row * 7 + group * 11 + offset * 13) % (usize::from(max as u8) + 1))
                                as i8
                        };
                        if (row + group + offset) % 2 == 0 {
                            magnitude
                        } else {
                            -magnitude
                        }
                    });
                    a8::Group {
                        mode,
                        exponent: base,
                        codes,
                        sse: 0.0,
                    }
                });
                a8::pack(&a8::Superblock {
                    base,
                    groups,
                    sse: 0.0,
                })
                .unwrap()
            })
            .collect();
        let bytes = encode_tensor(&rows).unwrap();
        let view = TensorView::new(&bytes).unwrap();
        let inputs: [[i8; 256]; 4] = [
            [-128; 256],
            [127; 256],
            [0; 256],
            std::array::from_fn(|i| (i as u8).wrapping_mul(73) as i8),
        ];
        let outputs = view.signed_i8_matvec_batch4(&inputs).unwrap();
        for (row, wire) in rows.iter().enumerate() {
            let (codes, exponents) = a8::unpack(wire).unwrap();
            for lane in 0..4 {
                let expected: i128 = (0..256)
                    .map(|i| {
                        i128::from(codes[i])
                            * i128::from(inputs[lane][i])
                            * (1i128 << (i32::from(exponents[i / 16]) + 24))
                    })
                    .sum();
                assert_eq!(outputs[row][lane], expected, "row {row}, lane {lane}");
            }
        }
    }

    #[test]
    fn eight_lane_batch_matches_independent_scalar_on_page_boundary() {
        let rows: Vec<Vec<u8>> = (0..257).map(make_row).collect();
        let encoded = encode_tensor(&rows).unwrap();
        let view = TensorView::new(&encoded).unwrap();
        let inputs: [[i8; 256]; 8] = [
            [-128; 256],
            [127; 256],
            [0; 256],
            [-1; 256],
            std::array::from_fn(|i| i as u8 as i8),
            std::array::from_fn(|i| (i as u8).wrapping_mul(41) as i8),
            std::array::from_fn(|i| if i % 2 == 0 { 127 } else { -128 }),
            std::array::from_fn(|i| if i % 3 == 0 { -1 } else { 1 }),
        ];
        let batch8 = view.signed_i8_matvec_batch8(&inputs).unwrap();
        let first4: [[i8; 256]; 4] = std::array::from_fn(|lane| inputs[lane]);
        let previous = view.signed_i8_matvec_batch4(&first4).unwrap();
        for (row, wire) in rows.iter().enumerate() {
            let (codes, exponents) = a8::unpack(wire).unwrap();
            assert_eq!(batch8[row][..4], previous[row]);
            for lane in 0..8 {
                let scalar: i128 = (0..256)
                    .map(|i| {
                        i128::from(codes[i])
                            * i128::from(inputs[lane][i])
                            * (1i128 << (i32::from(exponents[i / 16]) + 24))
                    })
                    .sum();
                assert_eq!(batch8[row][lane], scalar, "row {row} lane {lane}");
            }
        }
        let mut reused = vec![[0i128; 8]; view.len()];
        view.signed_i8_matvec_batch8_into(&inputs, &mut reused)
            .unwrap();
        assert_eq!(reused, batch8);
        let mut too_short = vec![[71i128; 8]; view.len() - 1];
        assert_eq!(
            view.signed_i8_matvec_batch8_into(&inputs, &mut too_short),
            Err(TensorError::WrongLength)
        );
        assert!(too_short.iter().all(|row| row.iter().all(|&x| x == 71)));
    }

    #[test]
    fn corrupt_directory_and_nested_page_fail_closed() {
        let source: Vec<Vec<u8>> = (0..257).map(make_row).collect();
        let original = encode_tensor(&source).unwrap();
        let mut bad = original.clone();
        bad[0] ^= 1;
        assert!(matches!(
            TensorView::new(&bad),
            Err(TensorError::InvalidHeader)
        ));
        let mut bad = original.clone();
        bad[8] = 3; // declared page count disagrees with row count
        assert!(matches!(
            TensorView::new(&bad),
            Err(TensorError::InvalidDirectory)
        ));
        let mut bad = original.clone();
        bad[12] ^= 1; // first page offset is no longer zero
        assert!(matches!(
            TensorView::new(&bad),
            Err(TensorError::InvalidDirectory)
        ));
        let mut bad = original.clone();
        bad[16] = 0; // second page offset becomes incorrect
        assert!(TensorView::new(&bad).is_err());
        let mut bad = original.clone();
        let nested_start = directory_bytes(source.len()).unwrap();
        bad[nested_start] ^= 1; // corrupt BA8P magic
        assert!(matches!(
            TensorView::new(&bad),
            Err(TensorError::InvalidPage)
        ));
        let mut bad = original.clone();
        bad.pop();
        assert!(TensorView::new(&bad).is_err());
        let mut bad = original.clone();
        bad.push(0);
        assert!(TensorView::new(&bad).is_err());
    }

    #[test]
    fn empty_oversized_and_bare_directory_rejected() {
        assert_eq!(directory_bytes(0), Err(TensorError::Empty));
        assert_eq!(
            directory_bytes(MAX_PAGES * ROWS_PER_PAGE + 1),
            Err(TensorError::TooManyPages)
        );
        assert!(matches!(encode_tensor(&[]), Err(TensorError::Empty)));
        assert!(TensorView::new(&[]).is_err());
    }

    #[test]
    fn full_page_storage_includes_both_index_levels() {
        assert_eq!(directory_bytes(1).unwrap(), 20);
        assert_eq!(directory_bytes(256).unwrap(), 20);
        assert_eq!(directory_bytes(257).unwrap(), 24);
        assert_eq!(paged::page_index_bytes(256).unwrap(), 72);
        assert_eq!(20 + 72, 92);
    }
}
