//! Versioned weight-page artifact used by the native RAM materializer.
//!
//! CRC32C detects accidental corruption only. It is not authenticity,
//! provenance, encryption, or a cryptographic content commitment.

use core::fmt;

use sml_core::{
    elastic_weights::WeightPrecision,
    PageId,
};

pub const WEIGHT_ARTIFACT_MAGIC: [u8; 8] = *b"SMLWGT01";
pub const WEIGHT_ARTIFACT_VERSION: u16 = 1;
pub const WEIGHT_ARTIFACT_HEADER_BYTES: usize = 64;

const FLAG_NONE: u32 = 0;
const CRC32C_POLYNOMIAL: u32 = 0x82f6_3b78;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeightArtifactHeader {
    pub page_id: PageId,
    pub precision: WeightPrecision,
    pub representation_version: u64,
    pub parameters: u64,
    pub payload_bytes: u64,
    pub payload_crc32c: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ParsedWeightArtifact<'a> {
    pub header: WeightArtifactHeader,
    pub payload: &'a [u8],
}

pub fn build_weight_artifact(
    page_id: PageId,
    representation_version: u64,
    precision: WeightPrecision,
    parameters: u64,
    payload: &[u8],
) -> Result<Vec<u8>, WeightArtifactError> {
    if parameters == 0 {
        return Err(WeightArtifactError::ZeroParameters);
    }
    let expected_payload = expected_payload_bytes(parameters, precision)?;
    let actual_payload =
        u64::try_from(payload.len()).map_err(|_| WeightArtifactError::ArithmeticOverflow)?;
    if actual_payload != expected_payload {
        return Err(WeightArtifactError::PayloadLength {
            expected: expected_payload,
            actual: actual_payload,
        });
    }

    let total = WEIGHT_ARTIFACT_HEADER_BYTES
        .checked_add(payload.len())
        .ok_or(WeightArtifactError::ArithmeticOverflow)?;
    let mut bytes = vec![0_u8; total];
    bytes[0..8].copy_from_slice(&WEIGHT_ARTIFACT_MAGIC);
    put_u16(&mut bytes, 8, WEIGHT_ARTIFACT_VERSION)?;
    put_u16(
        &mut bytes,
        10,
        u16::try_from(WEIGHT_ARTIFACT_HEADER_BYTES)
            .map_err(|_| WeightArtifactError::ArithmeticOverflow)?,
    )?;
    put_u32(&mut bytes, 12, page_id.0)?;
    bytes[16] = precision_code(precision);
    put_u64(&mut bytes, 24, representation_version)?;
    put_u64(&mut bytes, 32, parameters)?;
    put_u64(&mut bytes, 40, expected_payload)?;
    put_u32(&mut bytes, 48, crc32c(payload))?;
    let header_crc = crc32c(&bytes[..52]);
    put_u32(&mut bytes, 52, header_crc)?;
    put_u32(&mut bytes, 56, FLAG_NONE)?;
    put_u32(&mut bytes, 60, 0)?;
    bytes[WEIGHT_ARTIFACT_HEADER_BYTES..].copy_from_slice(payload);
    Ok(bytes)
}

pub fn parse_weight_artifact(bytes: &[u8]) -> Result<ParsedWeightArtifact<'_>, WeightArtifactError> {
    let header = bytes
        .get(..WEIGHT_ARTIFACT_HEADER_BYTES)
        .ok_or(WeightArtifactError::TruncatedHeader)?;
    if header[0..8] != WEIGHT_ARTIFACT_MAGIC {
        return Err(WeightArtifactError::InvalidMagic);
    }
    let version = get_u16(header, 8)?;
    if version != WEIGHT_ARTIFACT_VERSION {
        return Err(WeightArtifactError::UnsupportedVersion(version));
    }
    let header_bytes = usize::from(get_u16(header, 10)?);
    if header_bytes != WEIGHT_ARTIFACT_HEADER_BYTES {
        return Err(WeightArtifactError::InvalidHeaderBytes(header_bytes));
    }
    if header[17..24].iter().any(|byte| *byte != 0)
        || get_u32(header, 56)? != FLAG_NONE
        || get_u32(header, 60)? != 0
    {
        return Err(WeightArtifactError::UnsupportedFlagsOrReserved);
    }

    let precision = precision_from_code(header[16])?;
    let page_id = PageId(get_u32(header, 12)?);
    let representation_version = get_u64(header, 24)?;
    let parameters = get_u64(header, 32)?;
    if parameters == 0 {
        return Err(WeightArtifactError::ZeroParameters);
    }
    let payload_bytes = get_u64(header, 40)?;
    let expected_payload = expected_payload_bytes(parameters, precision)?;
    if payload_bytes != expected_payload {
        return Err(WeightArtifactError::PayloadLength {
            expected: expected_payload,
            actual: payload_bytes,
        });
    }

    let expected_header_crc = get_u32(header, 52)?;
    let actual_header_crc = crc32c(&header[..52]);
    if expected_header_crc != actual_header_crc {
        return Err(WeightArtifactError::HeaderChecksumMismatch {
            expected: expected_header_crc,
            actual: actual_header_crc,
        });
    }

    let payload_len =
        usize::try_from(payload_bytes).map_err(|_| WeightArtifactError::ArithmeticOverflow)?;
    let expected_file_len = WEIGHT_ARTIFACT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(WeightArtifactError::ArithmeticOverflow)?;
    if bytes.len() != expected_file_len {
        return Err(WeightArtifactError::FileLength {
            expected: expected_file_len,
            actual: bytes.len(),
        });
    }
    let payload = &bytes[WEIGHT_ARTIFACT_HEADER_BYTES..];
    let expected_payload_crc = get_u32(header, 48)?;
    let actual_payload_crc = crc32c(payload);
    if expected_payload_crc != actual_payload_crc {
        return Err(WeightArtifactError::PayloadChecksumMismatch {
            expected: expected_payload_crc,
            actual: actual_payload_crc,
        });
    }

    Ok(ParsedWeightArtifact {
        header: WeightArtifactHeader {
            page_id,
            precision,
            representation_version,
            parameters,
            payload_bytes,
            payload_crc32c: expected_payload_crc,
        },
        payload,
    })
}

pub(crate) fn expected_payload_bytes(
    parameters: u64,
    precision: WeightPrecision,
) -> Result<u64, WeightArtifactError> {
    let bits = (parameters as u128)
        .checked_mul(u128::from(precision.bits_per_parameter()))
        .ok_or(WeightArtifactError::ArithmeticOverflow)?;
    u64::try_from(bits.div_ceil(8)).map_err(|_| WeightArtifactError::ArithmeticOverflow)
}

pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (CRC32C_POLYNOMIAL & mask);
        }
    }
    !crc
}

const fn precision_code(precision: WeightPrecision) -> u8 {
    precision.bits_per_parameter()
}

fn precision_from_code(code: u8) -> Result<WeightPrecision, WeightArtifactError> {
    match code {
        1 => Ok(WeightPrecision::Boolean),
        2 => Ok(WeightPrecision::Ternary),
        4 => Ok(WeightPrecision::Residual4),
        other => Err(WeightArtifactError::InvalidPrecisionCode(other)),
    }
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), WeightArtifactError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), WeightArtifactError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), WeightArtifactError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_bytes(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), WeightArtifactError> {
    let end = offset
        .checked_add(value.len())
        .ok_or(WeightArtifactError::ArithmeticOverflow)?;
    bytes
        .get_mut(offset..end)
        .ok_or(WeightArtifactError::TruncatedHeader)?
        .copy_from_slice(value);
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, WeightArtifactError> {
    Ok(u16::from_le_bytes(get_array::<2>(bytes, offset)?))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, WeightArtifactError> {
    Ok(u32::from_le_bytes(get_array::<4>(bytes, offset)?))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, WeightArtifactError> {
    Ok(u64::from_le_bytes(get_array::<8>(bytes, offset)?))
}

fn get_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], WeightArtifactError> {
    let end = offset
        .checked_add(N)
        .ok_or(WeightArtifactError::ArithmeticOverflow)?;
    bytes
        .get(offset..end)
        .ok_or(WeightArtifactError::TruncatedHeader)?
        .try_into()
        .map_err(|_| WeightArtifactError::TruncatedHeader)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightArtifactError {
    TruncatedHeader,
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidHeaderBytes(usize),
    InvalidPrecisionCode(u8),
    UnsupportedFlagsOrReserved,
    ZeroParameters,
    PayloadLength { expected: u64, actual: u64 },
    HeaderChecksumMismatch { expected: u32, actual: u32 },
    PayloadChecksumMismatch { expected: u32, actual: u32 },
    FileLength { expected: usize, actual: usize },
    ArithmeticOverflow,
}

impl fmt::Display for WeightArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WeightArtifactError {}

#[cfg(test)]
mod tests {
    use super::{build_weight_artifact, crc32c, parse_weight_artifact, WeightArtifactError};
    use sml_core::{elastic_weights::WeightPrecision, PageId};

    #[test]
    fn crc32c_matches_castagnoli_check_vector() {
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    }

    #[test]
    fn boolean_artifact_round_trip_is_exact() {
        let bytes =
            build_weight_artifact(PageId(7), 3, WeightPrecision::Boolean, 9, &[0xaa, 0x01])
                .unwrap();
        let parsed = parse_weight_artifact(&bytes).unwrap();
        assert_eq!(parsed.header.page_id, PageId(7));
        assert_eq!(parsed.header.representation_version, 3);
        assert_eq!(parsed.header.precision, WeightPrecision::Boolean);
        assert_eq!(parsed.header.parameters, 9);
        assert_eq!(parsed.payload, &[0xaa, 0x01]);
    }

    #[test]
    fn payload_length_is_precision_aware() {
        assert_eq!(
            build_weight_artifact(PageId(1), 0, WeightPrecision::Residual4, 9, &[0; 4]),
            Err(WeightArtifactError::PayloadLength {
                expected: 5,
                actual: 4
            })
        );
    }
}
