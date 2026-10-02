use core::fmt;

use super::{HistoryError, ObservationEvent, SubjectKey};

/// Three u64 values, one u32, two Boolean digits and five tab separators.
pub const MAX_EVENT_BYTES: usize = 77;

/// Encode one typed claim, not an authenticated source or a verification receipt.
pub fn encode_event(event: ObservationEvent) -> Vec<u8> {
    let prior = event
        .predecessor()
        .map_or_else(|| String::from("-"), |id| id.to_string());
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        event.id(),
        event.key().subject(),
        event.key().group(),
        u8::from(event.bits()[0]),
        u8::from(event.bits()[1]),
        prior
    )
    .into_bytes()
}

/// Parse exactly six fields without allocating an unbounded column vector.
pub fn parse_event(bytes: &[u8], max_bytes: usize) -> Result<ObservationEvent, WireError> {
    if bytes.len() > max_bytes || bytes.len() > MAX_EVENT_BYTES || max_bytes == 0 {
        return Err(WireError::Limit);
    }
    let text = core::str::from_utf8(bytes).map_err(|_| WireError::Syntax)?;
    let mut fields = text.split('\t');
    let mut next = || fields.next().ok_or(WireError::Syntax);
    let id = unsigned(next()?)?;
    let subject = unsigned(next()?)?;
    let group = u32::try_from(unsigned(next()?)?).map_err(|_| WireError::Integer)?;
    let bits = [bit(next()?)?, bit(next()?)?];
    let predecessor = match next()? {
        "-" => None,
        field => Some(unsigned(field)?),
    };
    if fields.next().is_some() {
        return Err(WireError::Syntax);
    }
    let key = SubjectKey::new(subject, group).map_err(WireError::History)?;
    ObservationEvent::new(id, key, bits, predecessor).map_err(WireError::History)
}

fn unsigned(text: &str) -> Result<u64, WireError> {
    if text.is_empty()
        || text.len() > 20
        || (text.len() > 1 && text.starts_with('0'))
        || !text.as_bytes().iter().all(u8::is_ascii_digit)
    {
        return Err(WireError::Integer);
    }
    text.parse().map_err(|_| WireError::Integer)
}

fn bit(text: &str) -> Result<bool, WireError> {
    match text {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(WireError::Bit),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Limit,
    Syntax,
    Integer,
    Bit,
    History(HistoryError),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WireError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_every_payload_and_maximum_canonical_identities() {
        let key = SubjectKey::new(u64::MAX, u32::MAX).unwrap();
        for bits in [[false, false], [false, true], [true, false], [true, true]] {
            let event = ObservationEvent::new(u64::MAX, key, bits, Some(u64::MAX - 1));
            let event = event.unwrap();
            let bytes = encode_event(event);
            assert_eq!(bytes.len(), MAX_EVENT_BYTES);
            assert_eq!(parse_event(&bytes, MAX_EVENT_BYTES), Ok(event));
        }
    }

    #[test]
    fn rejects_noncanonical_numbers_whitespace_and_extra_fields() {
        let invalid: &[&[u8]] = &[
            b"01\t1\t0\t0\t1\t-",
            b"+1\t1\t0\t0\t1\t-",
            b" 1\t1\t0\t0\t1\t-",
            b"1\t01\t0\t0\t1\t-",
            b"1\t1\t00\t0\t1\t-",
            b"1\t1\t-0\t0\t1\t-",
            b"2\t1\t0\t0\t1\t01",
            b"1\t1\t0\t0\t1\t-\n",
            b"1\t1\t0\t0\t1\t-\r",
            b"1\t1\t0\t0\t1\t-\t",
            b"1\t1\t0\t0\t1",
            b"",
        ];
        for bytes in invalid {
            assert!(parse_event(bytes, MAX_EVENT_BYTES).is_err());
        }
    }

    #[test]
    fn integer_overflow_invalid_utf8_and_non_boolean_digits_reject() {
        let invalid: &[&[u8]] = &[
            b"18446744073709551616\t1\t0\t0\t1\t-",
            b"1\t1\t4294967296\t0\t1\t-",
            b"1\t1\t0\ttrue\t1\t-",
            b"1\t1\t0\t2\t1\t-",
            b"1\t1\t0\t00\t1\t-",
            b"1\t1\t0\t0\t1\t\xff",
            b"0\t1\t0\t0\t1\t-",
            b"1\t0\t0\t0\t1\t-",
            b"1\t1\t0\t0\t1\t0",
            b"1\t1\t0\t0\t1\t1",
        ];
        for bytes in invalid {
            assert!(parse_event(bytes, MAX_EVENT_BYTES).is_err());
        }
    }

    #[test]
    fn byte_limits_apply_before_decoding() {
        let bytes = b"1\t1\t0\t0\t1\t-";
        assert_eq!(parse_event(bytes, 0), Err(WireError::Limit));
        assert_eq!(parse_event(bytes, bytes.len() - 1), Err(WireError::Limit));
        assert!(parse_event(bytes, bytes.len()).is_ok());
        let oversized = vec![b'\t'; MAX_EVENT_BYTES + 1];
        assert_eq!(parse_event(&oversized, usize::MAX), Err(WireError::Limit));
    }

    #[test]
    fn exact_numeric_identities_do_not_accept_substring_equivalence() {
        let a = parse_event(b"7731\t1\t0\t0\t1\t-", MAX_EVENT_BYTES).unwrap();
        let b = parse_event(b"17731\t1\t0\t0\t1\t-", MAX_EVENT_BYTES).unwrap();
        assert_ne!(a.id(), b.id());
        assert_eq!(a.id(), 7731);
        assert_eq!(b.id(), 17731);
        assert_ne!(encode_event(a), encode_event(b));
    }
}
