//! Bounded little-endian SBG state trajectory exchange, schema sml.a4.trace/v1.
//! Structural validation is not authentication: valid payload edits need an
//! external expected-content comparison or authenticated transport to detect.
use crate::semantic::{SemanticBitField, SemanticState};
const MAGIC: &[u8; 8] = b"SMLA4TR1";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgTraceFrame<const S: usize> {
    pub depth: u32,
    pub bank_revision: u64,
    pub state: SemanticState<S>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbgTraceError {
    Header,
    Width,
    Length,
    Limit,
    Depth,
    Revision,
    Padding,
    Overflow,
    Allocation,
}
fn layout<const S: usize>(
    count: usize,
    max_frames: usize,
) -> Result<(usize, usize), SbgTraceError> {
    if S == 0 {
        return Err(SbgTraceError::Width);
    }
    if count > max_frames {
        return Err(SbgTraceError::Limit);
    }
    let record = S
        .div_ceil(64)
        .checked_mul(32)
        .and_then(|x| x.checked_add(12))
        .ok_or(SbgTraceError::Overflow)?;
    let bytes = count
        .checked_mul(record)
        .and_then(|x| x.checked_add(24))
        .ok_or(SbgTraceError::Overflow)?;
    Ok((record, bytes))
}
fn validate_sequence<const S: usize>(frames: &[SbgTraceFrame<S>]) -> Result<(), SbgTraceError> {
    let mut previous = 0;
    for (i, frame) in frames.iter().enumerate() {
        if usize::try_from(frame.depth).ok() != i.checked_add(1) {
            return Err(SbgTraceError::Depth);
        }
        if frame.bank_revision < previous {
            return Err(SbgTraceError::Revision);
        }
        previous = frame.bank_revision;
    }
    Ok(())
}
pub fn encode_sbg_trace<const S: usize>(
    frames: &[SbgTraceFrame<S>],
    max_frames: usize,
) -> Result<Vec<u8>, SbgTraceError> {
    let (_, bytes) = layout::<S>(frames.len(), max_frames)?;
    validate_sequence(frames)?;
    let mut out = Vec::new();
    out.try_reserve_exact(bytes)
        .map_err(|_| SbgTraceError::Allocation)?;
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(S as u64).to_le_bytes());
    out.extend_from_slice(&(frames.len() as u64).to_le_bytes());
    for frame in frames {
        out.extend_from_slice(&frame.depth.to_le_bytes());
        out.extend_from_slice(&frame.bank_revision.to_le_bytes());
        for field in [
            &frame.state.value,
            &frame.state.meaning_mask,
            &frame.state.ambiguity,
            &frame.state.routing,
        ] {
            for word in field.words() {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
    }
    Ok(out)
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, SbgTraceError> {
    let end = offset.checked_add(8).ok_or(SbgTraceError::Overflow)?;
    let word = bytes.get(offset..end).ok_or(SbgTraceError::Length)?;
    Ok(u64::from_le_bytes(
        word.try_into().map_err(|_| SbgTraceError::Length)?,
    ))
}
pub fn decode_sbg_trace<const S: usize>(
    bytes: &[u8],
    max_frames: usize,
) -> Result<Vec<SbgTraceFrame<S>>, SbgTraceError> {
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err(SbgTraceError::Header);
    }
    if read_u64(bytes, 8)? != S as u64 {
        return Err(SbgTraceError::Width);
    }
    let count = usize::try_from(read_u64(bytes, 16)?).map_err(|_| SbgTraceError::Overflow)?;
    let (record, len) = layout::<S>(count, max_frames)?;
    if len != bytes.len() {
        return Err(SbgTraceError::Length);
    }
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(count)
        .map_err(|_| SbgTraceError::Allocation)?;
    for payload in bytes[24..].chunks_exact(record) {
        let depth = u32::from_le_bytes(payload[..4].try_into().map_err(|_| SbgTraceError::Length)?);
        let revision = read_u64(payload, 4)?;
        let field_bytes = S.div_ceil(64) * 8;
        let read_field = |i: usize| -> Result<SemanticBitField<S>, SbgTraceError> {
            let start = 12 + i * field_bytes;
            let mut words = Vec::new();
            words
                .try_reserve_exact(S.div_ceil(64))
                .map_err(|_| SbgTraceError::Allocation)?;
            for chunk in payload[start..start + field_bytes].chunks_exact(8) {
                words.push(u64::from_le_bytes(
                    chunk.try_into().map_err(|_| SbgTraceError::Length)?,
                ));
            }
            SemanticBitField::from_words(words).map_err(|_| SbgTraceError::Padding)
        };
        frames.push(SbgTraceFrame {
            depth,
            bank_revision: revision,
            state: SemanticState {
                value: read_field(0)?,
                meaning_mask: read_field(1)?,
                ambiguity: read_field(2)?,
                routing: read_field(3)?,
            },
        });
    }
    validate_sequence(&frames)?;
    Ok(frames)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<SbgTraceFrame<65>> {
        (1..=3)
            .map(|depth| {
                let mut state = SemanticState::zeroed().unwrap();
                state.value.set(64, true).unwrap();
                state.meaning_mask.set(depth as usize, true).unwrap();
                state.ambiguity.set(0, true).unwrap();
                state.routing.set(63, true).unwrap();
                SbgTraceFrame {
                    depth,
                    bank_revision: u64::from(depth - 1),
                    state,
                }
            })
            .collect()
    }
    #[test]
    fn all_fields_and_non_word_aligned_width_round_trip() {
        let f = fixture();
        let b = encode_sbg_trace(&f, 3).unwrap();
        assert_eq!(b.len(), 24 + 3 * (12 + 64));
        assert_eq!(&b[..8], MAGIC);
        assert_eq!(decode_sbg_trace::<65>(&b, 3).unwrap(), f);
        assert_eq!(
            encode_sbg_trace(&decode_sbg_trace::<65>(&b, 3).unwrap(), 3).unwrap(),
            b
        );
    }
    #[test]
    fn rejects_truncation_trailing_data_headers_widths_and_allocation_claims() {
        let b = encode_sbg_trace(&fixture(), 3).unwrap();
        for n in 0..b.len() {
            assert!(decode_sbg_trace::<65>(&b[..n], 3).is_err());
        }
        let mut extra = b.clone();
        extra.push(0);
        assert_eq!(
            decode_sbg_trace::<65>(&extra, 3),
            Err(SbgTraceError::Length)
        );
        assert_eq!(decode_sbg_trace::<64>(&b, 3), Err(SbgTraceError::Width));
        assert_eq!(decode_sbg_trace::<65>(&b, 2), Err(SbgTraceError::Limit));
        let mut huge = b.clone();
        huge[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_sbg_trace::<65>(&huge, 3).is_err());
        let mut bad = b;
        bad[0] ^= 1;
        assert_eq!(decode_sbg_trace::<65>(&bad, 3), Err(SbgTraceError::Header));
    }
    #[test]
    fn rejects_bad_depth_revision_order_and_padding() {
        let mut f = fixture();
        f[1].depth = 1;
        assert_eq!(encode_sbg_trace(&f, 3), Err(SbgTraceError::Depth));
        let mut f = fixture();
        f[2].bank_revision = 0;
        assert_eq!(encode_sbg_trace(&f, 3), Err(SbgTraceError::Revision));
        let mut b = encode_sbg_trace(&fixture(), 3).unwrap();
        b[24 + 12 + 8] = 2;
        assert_eq!(decode_sbg_trace::<65>(&b, 3), Err(SbgTraceError::Padding));
        let mut b = encode_sbg_trace(&fixture(), 3).unwrap();
        b[24..28].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(decode_sbg_trace::<65>(&b, 3), Err(SbgTraceError::Depth));
    }
}
