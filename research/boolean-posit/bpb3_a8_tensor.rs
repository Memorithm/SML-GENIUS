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
            *output =
                a8::dot_exact(self.row(row)?, mask).map_err(|_| TensorError::InvalidPage)?;
        }
        Ok(())
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
