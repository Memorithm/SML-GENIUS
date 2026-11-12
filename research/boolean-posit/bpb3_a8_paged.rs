//! Page-local seek index for the variable-length BPB-3-A8 256-weight wire.
//! The index amortizes one checkpoint over eight independently decodable rows.
//! No weights, exponents, bitplanes, or numerical semantics are modified.
#![forbid(unsafe_code)]

#[path = "bpb3_a8_superblock.rs"]
#[allow(dead_code)]
mod a8;

const MAGIC: [u8; 4] = *b"BA8P";
pub const MAX_BLOCKS: usize = 256;
pub const CHECKPOINT_STRIDE: usize = 8;
const HEADER_BYTES: usize = 8;
const A8_FIXED_BYTES: usize = 41;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageError {
    Empty,
    TooManyBlocks,
    InvalidHeader,
    InvalidCheckpoint,
    InvalidBlock,
    WrongLength,
    IndexOutOfRange,
    Overflow,
}

/// The additional page-local bytes, excluding the original A8 wire payload.
/// An outer directory to locate variable-length pages is NOT included.
pub fn page_index_bytes(block_count: usize) -> Result<usize, PageError> {
    if block_count == 0 {
        return Err(PageError::Empty);
    }
    if block_count > MAX_BLOCKS {
        return Err(PageError::TooManyBlocks);
    }
    let checkpoints = block_count.div_ceil(CHECKPOINT_STRIDE);
    HEADER_BYTES
        .checked_add(checkpoints.checked_mul(2).ok_or(PageError::Overflow)?)
        .ok_or(PageError::Overflow)
}

/// Read the complete length of one A8 block from its fixed 5-byte
/// base/mode prefix. This does not validate the rest of the block.
fn a8_wire_length(payload: &[u8], offset: usize) -> Result<usize, PageError> {
    let prefix = payload
        .get(offset..offset.checked_add(5).ok_or(PageError::Overflow)?)
        .ok_or(PageError::WrongLength)?;
    let mode_bits = u32::from_le_bytes(
        prefix[1..5]
            .try_into()
            .map_err(|_| PageError::WrongLength)?,
    );
    let mut sum = 0usize;
    for group in 0..16 {
        let code = (mode_bits >> (2 * group)) & 3;
        if code == 3 {
            return Err(PageError::InvalidBlock);
        }
        sum = sum
            .checked_add((code as usize) + 1)
            .ok_or(PageError::Overflow)?;
    }
    let length = A8_FIXED_BYTES
        .checked_add(sum.checked_mul(2).ok_or(PageError::Overflow)?)
        .ok_or(PageError::Overflow)?;
    payload
        .get(offset..offset.checked_add(length).ok_or(PageError::Overflow)?)
        .ok_or(PageError::WrongLength)?;
    Ok(length)
}

/// Encode one self-contained page of 1..=256 canonical A8 wires. All
/// serialized bytes are charged to the page, including checkpoints.
/// The caller must account separately for any outer page directory.
pub fn encode_page(blocks: &[Vec<u8>]) -> Result<Vec<u8>, PageError> {
    let index_bytes = page_index_bytes(blocks.len())?;
    let mut payload_len = 0usize;
    let mut checkpoints = Vec::with_capacity((blocks.len() + 7) / 8);
    for (i, bytes) in blocks.iter().enumerate() {
        if i % CHECKPOINT_STRIDE == 0 {
            checkpoints.push(u16::try_from(payload_len).map_err(|_| PageError::Overflow)?);
        }
        if a8_wire_length(bytes, 0)? != bytes.len() || a8::unpack(bytes).is_err() {
            return Err(PageError::InvalidBlock);
        }
        payload_len = payload_len
            .checked_add(bytes.len())
            .ok_or(PageError::Overflow)?;
    }
    let payload_u16 = u16::try_from(payload_len).map_err(|_| PageError::Overflow)?;
    let count_u16 = u16::try_from(blocks.len()).map_err(|_| PageError::Overflow)?;
    let mut page = Vec::with_capacity(
        index_bytes
            .checked_add(payload_len)
            .ok_or(PageError::Overflow)?,
    );
    page.extend_from_slice(&MAGIC);
    page.extend_from_slice(&count_u16.to_le_bytes());
    page.extend_from_slice(&payload_u16.to_le_bytes());
    for checkpoint in checkpoints {
        page.extend_from_slice(&checkpoint.to_le_bytes());
    }
    for bytes in blocks {
        page.extend_from_slice(bytes);
    }
    Ok(page)
}

/// A fully validated, immutably borrowed A8 page. Constructor work is O(n)
/// over all blocks once; subsequent get() reads at most seven preceding
/// block headers (up to eight including the requested block).
pub struct ValidatedPage<'a> {
    raw: &'a [u8],
    payload_start: usize,
    count: usize,
}

impl<'a> ValidatedPage<'a> {
    pub fn new(raw: &'a [u8]) -> Result<Self, PageError> {
        let header = raw.get(..HEADER_BYTES).ok_or(PageError::WrongLength)?;
        if header[..4] != MAGIC {
            return Err(PageError::InvalidHeader);
        }
        let count = usize::from(u16::from_le_bytes([header[4], header[5]]));
        let index_bytes = page_index_bytes(count)?;
        let payload_len = usize::from(u16::from_le_bytes([header[6], header[7]]));
        let expected_len = index_bytes
            .checked_add(payload_len)
            .ok_or(PageError::Overflow)?;
        if raw.len() != expected_len {
            return Err(PageError::WrongLength);
        }
        let payload = &raw[index_bytes..];
        let mut cursor = 0usize;
        for i in 0..count {
            if i % CHECKPOINT_STRIDE == 0 {
                let position = HEADER_BYTES + (i / CHECKPOINT_STRIDE) * 2;
                let saved = usize::from(u16::from_le_bytes([raw[position], raw[position + 1]]));
                if saved != cursor {
                    return Err(PageError::InvalidCheckpoint);
                }
            }
            let size = a8_wire_length(payload, cursor)?;
            let wire = &payload[cursor..cursor + size];
            a8::unpack(wire).map_err(|_| PageError::InvalidBlock)?;
            cursor = cursor.checked_add(size).ok_or(PageError::Overflow)?;
        }
        if cursor != payload.len() {
            return Err(PageError::WrongLength);
        }
        Ok(Self {
            raw,
            payload_start: index_bytes,
            count,
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn get(&self, block: usize) -> Result<&'a [u8], PageError> {
        if block >= self.count {
            return Err(PageError::IndexOutOfRange);
        }
        let group = block / CHECKPOINT_STRIDE;
        let checkpoint_position = HEADER_BYTES + group * 2;
        let mut cursor = usize::from(u16::from_le_bytes([
            self.raw[checkpoint_position],
            self.raw[checkpoint_position + 1],
        ]));
        let payload = &self.raw[self.payload_start..];
        for _ in (group * CHECKPOINT_STRIDE)..block {
            cursor += a8_wire_length(payload, cursor)?;
        }
        let length = a8_wire_length(payload, cursor)?;
        Ok(&payload[cursor..cursor + length])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(index: usize) -> Vec<u8> {
        let base = (index % 52) as i8 - 24;
        let groups = std::array::from_fn(|group| {
            let mode = 1 + ((index + group) % 3) as u8;
            let mut codes = [0i8; 16];
            codes[(index + group) % 16] = if index % 2 == 0 { 1 } else { -1 };
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

    #[test]
    fn all_256_rows_seek_correctly_at_every_checkpoint_boundary() {
        let blocks: Vec<_> = (0..256).map(sample).collect();
        let serialized = encode_page(&blocks).unwrap();
        let page = ValidatedPage::new(&serialized).unwrap();
        assert_eq!(page.len(), 256);
        for (i, original) in blocks.iter().enumerate() {
            assert_eq!(page.get(i).unwrap(), original);
        }
        assert_eq!(page.get(256), Err(PageError::IndexOutOfRange));
        assert_eq!(
            serialized.len(),
            blocks.iter().map(Vec::len).sum::<usize>() + 72
        );
    }

    #[test]
    fn all_short_page_sizes_and_single_row_work() {
        for count in [1, 7, 8, 9, 15, 16, 17, 63, 64, 65] {
            let blocks: Vec<_> = (0..count).map(sample).collect();
            let bytes = encode_page(&blocks).unwrap();
            let page = ValidatedPage::new(&bytes).unwrap();
            for (i, original) in blocks.iter().enumerate() {
                assert_eq!(page.get(i).unwrap(), original);
            }
            assert_eq!(
                bytes.len() - blocks.iter().map(Vec::len).sum::<usize>(),
                page_index_bytes(count).unwrap()
            );
        }
    }

    #[test]
    fn invalid_header_checkpoint_and_wire_fail_closed() {
        let blocks: Vec<_> = (0..17).map(sample).collect();
        let good = encode_page(&blocks).unwrap();
        let mut corrupt = good.clone();
        corrupt[0] ^= 1;
        assert!(matches!(
            ValidatedPage::new(&corrupt),
            Err(PageError::InvalidHeader)
        ));
        let mut corrupt = good.clone();
        corrupt[8] = 1;
        assert!(matches!(
            ValidatedPage::new(&corrupt),
            Err(PageError::InvalidCheckpoint)
        ));
        let mut corrupt = good.clone();
        let payload_start = page_index_bytes(blocks.len()).unwrap();
        corrupt[payload_start + 1] |= 3;
        assert!(matches!(
            ValidatedPage::new(&corrupt),
            Err(PageError::InvalidBlock)
        ));
        let mut corrupt = good.clone();
        corrupt.pop();
        assert!(matches!(
            ValidatedPage::new(&corrupt),
            Err(PageError::WrongLength)
        ));
        let mut corrupt = good.clone();
        corrupt.extend_from_slice(&[0]);
        assert!(matches!(
            ValidatedPage::new(&corrupt),
            Err(PageError::WrongLength)
        ));
    }

    #[test]
    fn indexed_page_rejects_noncanonical_or_out_of_range_inputs() {
        assert_eq!(page_index_bytes(0), Err(PageError::Empty));
        assert_eq!(page_index_bytes(257), Err(PageError::TooManyBlocks));
        assert!(matches!(encode_page(&[]), Err(PageError::Empty)));
        let many = vec![sample(0); 257];
        assert!(matches!(encode_page(&many), Err(PageError::TooManyBlocks)));
        let mut bad = sample(1);
        bad[1] |= 3;
        assert!(matches!(encode_page(&[bad]), Err(PageError::InvalidBlock)));
    }

    #[test]
    fn indexing_is_page_local_and_not_free() {
        assert_eq!(page_index_bytes(1).unwrap(), 10);
        assert_eq!(page_index_bytes(8).unwrap(), 10);
        assert_eq!(page_index_bytes(9).unwrap(), 12);
        assert_eq!(page_index_bytes(256).unwrap(), 72);
        assert!((108.0 + 72.0 / 256.0 - 108.28125_f64).abs() < f64::EPSILON);
    }
}
