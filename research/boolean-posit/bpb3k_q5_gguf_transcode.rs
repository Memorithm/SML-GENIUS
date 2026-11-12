//! Strict, independent GGUF v3 source-precision to experimental Q5-178 converter.
//! Not readable by stock ggml: experimental tensor type intentionally invalid.
//! Only entire 2D+ FP32/FP16/BF16 rows divisible by 256 are converted.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_q5_source_quantizer.rs"]
#[allow(dead_code)]
mod quant;

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

// Invalid ggml type (not the standard Q5_K=13); stock loaders must fail.
const Q5_EXPERIMENTAL_TYPE: u32 = 0xffff_0005;
const F32: u32 = 0;
const F16: u32 = 1;
const BF16: u32 = 30;
const MAX_GGUF: u64 = 600_000_000;
const MAX_ENTRIES: u64 = 200_000;
const MAX_STRING: u64 = 2_000_000;

#[derive(Debug)]
struct Tensor {
    name: String,
    kind: u32,
    kind_field: u64,
    values: u64,
    dims: u32,
    first_dim: u64,
    offset: u64,
    offset_field: u64,
}
fn read<const N: usize>(f: &mut File) -> Result<[u8; N], String> {
    let mut out = [0u8; N];
    f.read_exact(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
fn u32le(f: &mut File) -> Result<u32, String> {
    Ok(u32::from_le_bytes(read(f)?))
}
fn u64le(f: &mut File) -> Result<u64, String> {
    Ok(u64::from_le_bytes(read(f)?))
}
fn advance(f: &mut File, n: u64, limit: u64) -> Result<(), String> {
    let pos = f.stream_position().map_err(|e| e.to_string())?;
    let end = pos.checked_add(n).ok_or("STOP_GGUF_OFFSET_OVERFLOW")?;
    if end > limit {
        return Err("STOP_GGUF_TRUNCATED".into());
    }
    f.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
    Ok(())
}
fn read_string(f: &mut File, limit: u64) -> Result<String, String> {
    let n = u64le(f)?;
    if n > MAX_STRING {
        return Err("STOP_LONG_STRING".into());
    }
    if f.stream_position()
        .map_err(|e| e.to_string())?
        .checked_add(n)
        .ok_or("STOP_STRING_OFFSET_OVERFLOW")?
        > limit
    {
        return Err("STOP_TRUNCATED_STRING".into());
    }
    let mut bytes = vec![0u8; n as usize];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "STOP_BAD_UTF8".into())
}
fn skip_value(f: &mut File, ty: u32, limit: u64) -> Result<(), String> {
    match ty {
        0 | 1 | 7 => advance(f, 1, limit),
        2 | 3 => advance(f, 2, limit),
        4 | 5 | 6 => advance(f, 4, limit),
        10..=12 => advance(f, 8, limit),
        8 => {
            let n = u64le(f)?;
            advance(f, n, limit)
        }
        9 => {
            let elem = u32le(f)?;
            let count = u64le(f)?;
            if count > 1_000_000 {
                return Err("STOP_HUGE_METADATA_ARRAY".into());
            }
            if elem == 8 {
                for _ in 0..count {
                    skip_value(f, elem, limit)?;
                }
                Ok(())
            } else {
                let width = match elem {
                    0 | 1 | 7 => 1,
                    2 | 3 => 2,
                    4 | 5 | 6 => 4,
                    10..=12 => 8,
                    _ => return Err("STOP_UNSUPPORTED_METADATA_ELEMENT".into()),
                };
                advance(
                    f,
                    count.checked_mul(width).ok_or("STOP_METADATA_OVERFLOW")?,
                    limit,
                )
            }
        }
        _ => Err("STOP_UNKNOWN_METADATA_TYPE".into()),
    }
}
fn tensors(f: &mut File, file_len: u64) -> Result<(u64, u64, Vec<Tensor>), String> {
    if read::<4>(f)? != *b"GGUF" {
        return Err("STOP_NOT_GGUF".into());
    }
    if u32le(f)? != 3 {
        return Err("STOP_UNSUPPORTED_GGUF_VERSION".into());
    }
    let n_tensors = u64le(f)?;
    let n_kvs = u64le(f)?;
    if n_tensors > MAX_ENTRIES || n_kvs > MAX_ENTRIES {
        return Err("STOP_EXCESSIVE_GGUF_ENTRIES".into());
    }
    let mut alignment = 32u64;
    for _ in 0..n_kvs {
        let key = read_string(f, file_len)?;
        let ty = u32le(f)?;
        if key == "general.alignment" {
            alignment = match ty {
                4 => u64::from(u32le(f)?),
                10 => u64le(f)?,
                _ => return Err("STOP_ALIGNMENT_TYPE".into()),
            };
        } else {
            skip_value(f, ty, file_len)?;
        }
    }
    if alignment == 0 || alignment > 4096 || !alignment.is_power_of_two() {
        return Err("STOP_ALIGNMENT_RANGE".into());
    }
    let mut entries = Vec::new();
    for _ in 0..n_tensors {
        let name = read_string(f, file_len)?;
        let dims = u32le(f)?;
        if !(1..=4).contains(&dims) {
            return Err("STOP_DIMS".into());
        }
        let mut values = 1u64;
        let mut first_dim = 0;
        for dimension in 0..dims {
            let extent = u64le(f)?;
            if extent == 0 {
                return Err("STOP_ZERO_DIMENSION".into());
            }
            if dimension == 0 {
                first_dim = extent;
            }
            values = values.checked_mul(extent).ok_or("STOP_VALUES_OVERFLOW")?;
        }
        let kind_field = f.stream_position().map_err(|e| e.to_string())?;
        let kind = u32le(f)?;
        let offset_field = f.stream_position().map_err(|e| e.to_string())?;
        let offset = u64le(f)?;
        entries.push(Tensor {
            name,
            kind,
            kind_field,
            values,
            dims,
            first_dim,
            offset,
            offset_field,
        });
    }
    let header = f.stream_position().map_err(|e| e.to_string())?;
    let start = header
        .checked_add(alignment - 1)
        .ok_or("STOP_ALIGN_OVERFLOW")?
        & !(alignment - 1);
    if start > file_len {
        return Err("STOP_TRUNCATED_TENSOR_DATA".into());
    }
    Ok((start, alignment, entries))
}

/// A row-multiple source-precision matrix, not a vector/norm/embedding
/// whose first dimension cannot be represented by whole 256-value blocks.
fn should_quantize(tensor: &Tensor) -> bool {
    tensor.dims >= 2
        && tensor.first_dim % 256 == 0
        && tensor.values % 256 == 0
        && matches!(tensor.kind, F16 | BF16 | F32)
}

fn source_width(kind: u32) -> Option<u64> {
    match kind {
        F16 | BF16 => Some(2),
        F32 => Some(4),
        _ => None,
    }
}

fn encode_source_block(input: &mut File, kind: u32) -> Result<[u8; 178], String> {
    match kind {
        F16 | BF16 => {
            let bytes = read::<512>(input)?;
            let codes =
                std::array::from_fn(|i| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]));
            let converted = if kind == F16 {
                quant::quantize_fp16(&codes)
            } else {
                quant::quantize_bf16(&codes)
            };
            converted
                .map(|x| x.bytes)
                .map_err(|e| format!("STOP_Q5_SOURCE_{e:?}"))
        }
        F32 => {
            let bytes = read::<1024>(input)?;
            let codes = std::array::from_fn(|i| {
                f32::from_bits(u32::from_le_bytes([
                    bytes[4 * i],
                    bytes[4 * i + 1],
                    bytes[4 * i + 2],
                    bytes[4 * i + 3],
                ]))
            });
            quant::quantize_f32(&codes)
                .map(|x| x.bytes)
                .map_err(|e| format!("STOP_Q5_SOURCE_{e:?}"))
        }
        _ => Err("STOP_Q5_UNEXPECTED_SOURCE_TYPE".into()),
    }
}

/// Copy a GGUF v3 envelope and write an explicitly nonstandard, fail-closed
/// experimental payload. Retains all non-matrix payloads byte-for-byte and
/// rewrites type/offset header fields only for converted matrices.
fn transcode(source: &Path, dest: &Path) -> Result<(u64, u64), String> {
    if source == dest {
        return Err("STOP_SOURCE_DEST_SAME".into());
    }
    if dest.exists() {
        return Err("STOP_DEST_EXISTS".into());
    }
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let file_len = input.metadata().map_err(|e| e.to_string())?.len();
    if !(1..=MAX_GGUF).contains(&file_len) {
        return Err("STOP_GGUF_FILE_SIZE".into());
    }
    let (start, alignment, mut entries) = tensors(&mut input, file_len)?;
    if entries.is_empty() {
        return Err("STOP_NO_TENSORS".into());
    }
    entries.sort_by_key(|t| t.offset);
    if entries[0].offset != 0 {
        return Err("STOP_FIRST_TENSOR_NOT_AT_DATA_START".into());
    }
    for adjacent in entries.windows(2) {
        if adjacent[1].offset <= adjacent[0].offset || adjacent[1].offset % alignment != 0 {
            return Err("STOP_OVERLAP_OR_MISALIGNMENT".into());
        }
    }
    let mut selected = 0u64;
    // Validate selected tensor payload sizes before ever creating an output.
    for (i, tensor) in entries.iter().enumerate() {
        let from = start
            .checked_add(tensor.offset)
            .ok_or("STOP_SOURCE_OFFSET_OVERFLOW")?;
        let end = if let Some(next) = entries.get(i + 1) {
            start
                .checked_add(next.offset)
                .ok_or("STOP_NEXT_OFFSET_OVERFLOW")?
        } else {
            file_len
        };
        if end <= from || end > file_len {
            return Err("STOP_TENSOR_OUTSIDE_SOURCE".into());
        }
        if should_quantize(tensor) {
            let expected = tensor
                .values
                .checked_mul(source_width(tensor.kind).unwrap())
                .ok_or("STOP_Q5_SOURCE_SIZE_OVERFLOW")?;
            let available = end - from;
            if expected > available || available - expected >= alignment {
                return Err(format!("STOP_Q5_SOURCE_SIZE name={}", tensor.name));
            }
            selected += 1;
        }
    }
    if selected == 0 {
        return Err("STOP_NO_ELIGIBLE_SOURCE_MATRICES".into());
    }

    let result = (|| {
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dest)
            .map_err(|e| e.to_string())?;
        input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let copied = std::io::copy(
            &mut std::io::Read::by_ref(&mut input).take(start),
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        if copied != start {
            return Err("STOP_HEADER_SHORT_COPY".into());
        }

        let mut converted_blocks = 0u64;
        let mut converted_tensors = 0u64;
        for (i, tensor) in entries.iter().enumerate() {
            let output_pos = output.stream_position().map_err(|e| e.to_string())?;
            if output_pos < start || (output_pos - start) % alignment != 0 {
                return Err("STOP_Q5_OUTPUT_ALIGNMENT".into());
            }
            let offset = output_pos - start;
            output
                .seek(SeekFrom::Start(tensor.offset_field))
                .map_err(|e| e.to_string())?;
            output
                .write_all(&offset.to_le_bytes())
                .map_err(|e| e.to_string())?;

            if should_quantize(tensor) {
                output
                    .seek(SeekFrom::Start(tensor.kind_field))
                    .map_err(|e| e.to_string())?;
                output
                    .write_all(&Q5_EXPERIMENTAL_TYPE.to_le_bytes())
                    .map_err(|e| e.to_string())?;
            }
            output.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
            let from = start
                .checked_add(tensor.offset)
                .ok_or("STOP_SOURCE_OFFSET_OVERFLOW")?;
            let end = if let Some(next) = entries.get(i + 1) {
                start
                    .checked_add(next.offset)
                    .ok_or("STOP_NEXT_OFFSET_OVERFLOW")?
            } else {
                file_len
            };
            input
                .seek(SeekFrom::Start(from))
                .map_err(|e| e.to_string())?;
            if should_quantize(tensor) {
                let blocks = tensor.values / 256;
                for _ in 0..blocks {
                    let encoded = encode_source_block(&mut input, tensor.kind)?;
                    output.write_all(&encoded).map_err(|e| e.to_string())?;
                }
                converted_blocks += blocks;
                converted_tensors += 1;
                println!(
                    "Q5_178_SOURCE_TENSOR name={} source_type={} blocks={} output_bytes={}",
                    tensor.name,
                    tensor.kind,
                    blocks,
                    blocks * 178
                );
            } else {
                let copied = std::io::copy(
                    &mut std::io::Read::by_ref(&mut input).take(end - from),
                    &mut output,
                )
                .map_err(|e| e.to_string())?;
                if copied != end - from {
                    return Err("STOP_UNCHANGED_TENSOR_SHORT_COPY".into());
                }
            }
            if i + 1 < entries.len() {
                let pos = output.stream_position().map_err(|e| e.to_string())?;
                let pad = (alignment - (pos - start) % alignment) % alignment;
                output
                    .write_all(&[0u8; 4096][..pad as usize])
                    .map_err(|e| e.to_string())?;
            }
            if output.stream_position().map_err(|e| e.to_string())? > 1_000_000_000 {
                return Err("STOP_Q5_OUTPUT_TOO_LARGE".into());
            }
        }
        output.sync_all().map_err(|e| e.to_string())?;
        println!(
            "PASS_Q5_178_GGUF_SOURCE tensors={} blocks={} bytes_per_block=178 type=0xffff0005 standard_loader=false",
            converted_tensors, converted_blocks
        );
        Ok((converted_tensors, converted_blocks))
    })();
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

/// Prepare only for an exact-source-pinned patched ggml process: translate
/// the deliberately nonstandard Q5 sentinel to the Q3_K surrogate type ID.
/// Never mutate the safe sentinel source; reject mixed stock Q3_K matrices.
/// The patched runtime must override the Q3_K byte stride to 178 and
/// replace its dequantization, tiled unpacker and vector dot implementations.
fn prepare_for_patched_runtime(source: &Path, dest: &Path) -> Result<u64, String> {
    if source == dest {
        return Err("STOP_PREPARE_SOURCE_DEST_SAME".into());
    }
    if dest.exists() {
        return Err("STOP_PREPARE_DEST_EXISTS".into());
    }
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let len = input.metadata().map_err(|e| e.to_string())?.len();
    if !(1..=MAX_GGUF).contains(&len) {
        return Err("STOP_PREPARE_FILE_SIZE".into());
    }
    let (start, alignment, mut entries) = tensors(&mut input, len)?;
    entries.sort_by_key(|t| t.offset);
    if entries.is_empty() || entries[0].offset != 0 {
        return Err("STOP_PREPARE_TENSOR_DIRECTORY".into());
    }
    let mut count = 0;
    for (index, tensor) in entries.iter().enumerate() {
        if index > 0
            && (tensor.offset <= entries[index - 1].offset || tensor.offset % alignment != 0)
        {
            return Err("STOP_PREPARE_OFFSETS".into());
        }
        if tensor.kind == 11 {
            return Err("STOP_PREPARE_MIXED_OR_ALREADY_Q3".into());
        }
        if tensor.kind != Q5_EXPERIMENTAL_TYPE {
            continue;
        }
        if tensor.dims < 2 || tensor.first_dim % 256 != 0 || tensor.values % 256 != 0 {
            return Err("STOP_PREPARE_INVALID_Q5_SHAPE".into());
        }
        let expected = (tensor.values / 256)
            .checked_mul(178)
            .ok_or("STOP_PREPARE_Q5_SIZE_OVERFLOW")?;
        let from = start
            .checked_add(tensor.offset)
            .ok_or("STOP_PREPARE_OFFSET_OVERFLOW")?;
        let end = if let Some(next) = entries.get(index + 1) {
            start
                .checked_add(next.offset)
                .ok_or("STOP_PREPARE_NEXT_OVERFLOW")?
        } else {
            len
        };
        if from >= end || end > len || expected > end - from || end - from - expected >= alignment {
            return Err("STOP_PREPARE_Q5_SIZE".into());
        }
        count += 1;
    }
    if count == 0 {
        return Err("STOP_PREPARE_NO_Q5_TENSORS".into());
    }
    let result = (|| {
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dest)
            .map_err(|e| e.to_string())?;
        input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        if copied != len {
            return Err("STOP_PREPARE_SHORT_COPY".into());
        }
        for tensor in entries.iter().filter(|t| t.kind == Q5_EXPERIMENTAL_TYPE) {
            output
                .seek(SeekFrom::Start(tensor.kind_field))
                .map_err(|e| e.to_string())?;
            output
                .write_all(&11u32.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        println!(
            "PASS_Q5_178_PREPARE_PATCHED_RUNTIME tensors={} q3_surrogate_type=11 payload_bytes=178 stock_loader_forbidden=true",
            count
        );
        Ok(count)
    })();
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args.as_slice() {
        [_, mode, source, dest] if mode == "transcode" => {
            transcode(Path::new(source), Path::new(dest))?;
            Ok(())
        }
        [_, mode, source, dest] if mode == "prepare-patched-runtime" => {
            prepare_for_patched_runtime(Path::new(source), Path::new(dest))?;
            Ok(())
        }
        _ => Err("USAGE: bpb3k_q5_gguf_transcode transcode <source-F16-BF16-F32-gguf> <experimental-Q5-178-gguf> | prepare-patched-runtime <experimental-Q5-178-gguf> <temporary-patched-only-gguf>".into()),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static TEST_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_root() -> std::path::PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("mem-q5-gguf-fixture-{}-{id}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        root
    }

    fn add_tensor(
        header: &mut Vec<u8>,
        name: &str,
        kind: u32,
        dims: &[u64],
        offset: u64,
    ) -> (usize, usize) {
        header.extend_from_slice(&(name.len() as u64).to_le_bytes());
        header.extend_from_slice(name.as_bytes());
        header.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for &d in dims {
            header.extend_from_slice(&d.to_le_bytes());
        }
        let kind_at = header.len();
        header.extend_from_slice(&kind.to_le_bytes());
        let offset_at = header.len();
        header.extend_from_slice(&offset.to_le_bytes());
        (kind_at, offset_at)
    }

    fn fixture() -> (Vec<u8>, Vec<(usize, usize)>, Vec<Vec<u8>>) {
        let mut header = Vec::new();
        header.extend_from_slice(b"GGUF");
        header.extend_from_slice(&3u32.to_le_bytes());
        header.extend_from_slice(&5u64.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes());
        let fields = vec![
            add_tensor(&mut header, "f16.matrix", F16, &[256, 2], 0),
            add_tensor(&mut header, "bf16.matrix", BF16, &[256, 1], 1024),
            add_tensor(&mut header, "f32.matrix", F32, &[256, 1], 1536),
            add_tensor(&mut header, "q3.untouched", 11, &[256], 2560),
            add_tensor(&mut header, "f16.vector", F16, &[16], 2688),
        ];
        header.resize((header.len() + 31) & !31, 0);
        let fp16: Vec<u8> = (0..512usize)
            .flat_map(|i| [0x3c00u16, 0xbc00, 0x3800, 0x3400][i % 4].to_le_bytes())
            .collect();
        let bf16: Vec<u8> = (0..256usize)
            .flat_map(|i| [0x3f80u16, 0xbf80, 0x3f00, 0x3e80][i % 4].to_le_bytes())
            .collect();
        let f32: Vec<u8> = (0..256usize)
            .flat_map(|i| ((i as f32 - 130.0) / 9.0).to_le_bytes())
            .collect();
        let mut stock = (0..110usize)
            .map(|i| (i.wrapping_mul(79) + 17) as u8)
            .collect::<Vec<u8>>();
        stock.resize(128, 0);
        let vector = (0..16usize)
            .flat_map(|i| [0x3c00u16, 0xbc00][i % 2].to_le_bytes())
            .collect::<Vec<u8>>();
        let chunks = vec![fp16, bf16, f32, stock, vector];
        let mut raw = header;
        for chunk in &chunks {
            raw.extend_from_slice(chunk);
        }
        (raw, fields, chunks)
    }

    #[test]
    fn real_gguf_v3_source_weights_quantized_and_offsets_relocated() {
        let root = temp_root();
        let src = root.join("original.gguf");
        let dst = root.join("experimental.gguf");
        let (original, fields, chunks) = fixture();
        fs::write(&src, &original).unwrap();
        let (tensors_done, blocks_done) = transcode(&src, &dst).unwrap();
        assert_eq!((tensors_done, blocks_done), (3, 4));
        let output = fs::read(&dst).unwrap();
        let mut inp = File::open(&dst).unwrap();
        let (start, alignment, entries) = tensors(&mut inp, output.len() as u64).unwrap();
        assert_eq!(alignment, 32);
        assert_eq!(
            entries.iter().map(|t| t.offset).collect::<Vec<_>>(),
            vec![0, 384, 576, 768, 896]
        );
        assert_eq!(
            entries.iter().map(|t| t.kind).collect::<Vec<_>>(),
            vec![
                Q5_EXPERIMENTAL_TYPE,
                Q5_EXPERIMENTAL_TYPE,
                Q5_EXPERIMENTAL_TYPE,
                11,
                F16
            ]
        );

        let f16_values: Vec<u16> = chunks[0]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let bf16_values: Vec<u16> = chunks[1]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let fp32_values: Vec<f32> = chunks[2]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let f16_expected = quant::quantize_fp16_tensor(&f16_values).unwrap();
        let bf16_expected = quant::quantize_bf16_tensor(&bf16_values).unwrap();
        let f32_expected = quant::quantize_f32_tensor(&fp32_values).unwrap();
        let at = start as usize;
        assert_eq!(&output[at..at + 356], &f16_expected.payload);
        assert_eq!(&output[at + 384..at + 562], &bf16_expected.payload);
        assert_eq!(&output[at + 576..at + 754], &f32_expected.payload);
        assert_eq!(&output[at + 768..at + 896], &chunks[3]);
        assert_eq!(&output[at + 896..at + 928], &chunks[4]);
        for (index, &(kind_at, off_at)) in fields.iter().enumerate() {
            let kind = u32::from_le_bytes(output[kind_at..kind_at + 4].try_into().unwrap());
            let off = u64::from_le_bytes(output[off_at..off_at + 8].try_into().unwrap());
            assert_eq!(kind, entries[index].kind);
            assert_eq!(off, entries[index].offset);
        }
        assert_eq!(fs::read(&src).unwrap(), original);
        assert!(transcode(&src, &dst).is_err()); // never clobber existing output
        assert_eq!(fs::read(&dst).unwrap(), output);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reject_nonfinite_matrix_without_creating_partial_output() {
        let root = temp_root();
        let src = root.join("nan.gguf");
        let dst = root.join("bad.gguf");
        let (mut raw, _, _) = fixture();
        let header_len = (raw.len() - (1024 + 512 + 1024 + 128 + 32)) as usize;
        // One NaN in the *second* F16 source block.
        let index = header_len + 2 * 280;
        raw[index..index + 2].copy_from_slice(&0x7e01u16.to_le_bytes());
        fs::write(&src, raw).unwrap();
        let error = transcode(&src, &dst).unwrap_err();
        assert!(error.contains("STOP_Q5_SOURCE_NonFiniteSource"), "{error}");
        assert!(!dst.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reject_malformed_or_ineligible_gguf_before_output() {
        let root = temp_root();
        let src = root.join("original.gguf");
        let dst = root.join("converted.gguf");
        let (mut raw, fields, _) = fixture();
        raw[0..4].copy_from_slice(b"BAD!");
        fs::write(&src, &raw).unwrap();
        assert!(transcode(&src, &dst).unwrap_err().contains("STOP_NOT_GGUF"));
        assert!(!dst.exists());
        raw[0..4].copy_from_slice(b"GGUF");
        // Invalid source matrix dimension, zero forbidden.
        let first_dim_at = fields[0].0 - 16;
        raw[first_dim_at..first_dim_at + 8].copy_from_slice(&0u64.to_le_bytes());
        fs::write(&src, &raw).unwrap();
        assert!(transcode(&src, &dst)
            .unwrap_err()
            .contains("STOP_ZERO_DIMENSION"));
        assert!(!dst.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn q5_preparation_rewrites_only_experimental_type_fields_after_validation() {
        let root = temp_root();
        let original_file = root.join("original.gguf");
        let experimental_file = root.join("experimental.gguf");
        let prepared_file = root.join("prepared.gguf");
        let (mut source, fields, _) = fixture();
        // Replace the fixture's original 110-byte Q3 tensor with a
        // well-formed, unquantized 64-value FP16 vector (128 bytes).
        let kind_field = fields[3].0;
        source[kind_field..kind_field + 4].copy_from_slice(&F16.to_le_bytes());
        source[kind_field - 8..kind_field].copy_from_slice(&64u64.to_le_bytes());
        fs::write(&original_file, source).unwrap();
        transcode(&original_file, &experimental_file).unwrap();
        let safe = fs::read(&experimental_file).unwrap();
        assert_eq!(
            prepare_for_patched_runtime(&experimental_file, &prepared_file).unwrap(),
            3
        );
        let prepared = fs::read(&prepared_file).unwrap();
        assert_eq!(safe.len(), prepared.len());
        let mut reconstructed = safe.clone();
        for &(field, _) in &fields[..3] {
            reconstructed[field..field + 4].copy_from_slice(&11u32.to_le_bytes());
        }
        assert_eq!(prepared, reconstructed);
        assert_eq!(fs::read(&experimental_file).unwrap(), safe);
        assert_eq!(
            prepare_for_patched_runtime(&prepared_file, &root.join("second.gguf")).unwrap_err(),
            "STOP_PREPARE_MIXED_OR_ALREADY_Q3"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn q5_preparation_rejects_mixed_original_q3_and_preserves_safe_input() {
        let root = temp_root();
        let original_file = root.join("original.gguf");
        let experimental_file = root.join("experimental.gguf");
        let prepared_file = root.join("prepared.gguf");
        let (raw, _, _) = fixture();
        fs::write(&original_file, raw).unwrap();
        transcode(&original_file, &experimental_file).unwrap();
        let source = fs::read(&experimental_file).unwrap();
        assert_eq!(
            prepare_for_patched_runtime(&experimental_file, &prepared_file).unwrap_err(),
            "STOP_PREPARE_MIXED_OR_ALREADY_Q3"
        );
        assert!(!prepared_file.exists());
        assert_eq!(fs::read(&experimental_file).unwrap(), source);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_conversion_to_standard_q5_k_or_q3_k_identifiers() {
        assert_ne!(Q5_EXPERIMENTAL_TYPE, 13);
        assert_ne!(Q5_EXPERIMENTAL_TYPE, 11);
        assert_eq!(Q5_EXPERIMENTAL_TYPE, 0xffff_0005);
    }
}
