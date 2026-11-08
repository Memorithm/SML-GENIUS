//! BPB3-K CPU kernels: scalar reference, AVX2 SIMD, 4-row tiled, hybrid SIMD×tiled.
//! Research-only kernel, Rust 1.89, no vendor GPU SDK. Direct 110-byte bitplane
//! Q3_K relayout with 292-byte Q8_K activations. Never accepts stock Q3_K bytes.
//! The eight-lane integer / fp32 reduction mirrors ggml's Q3_K generic oracle.
use std::env;
use std::ffi::c_void;
use std::slice;
use std::sync::OnceLock;

const BLOCK_W: usize = 110;
const BLOCK_A: usize = 292;
const LANES: usize = 8;
const TILE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode { Scalar, Simd, Tiled, Hybrid, Fused }

fn mode() -> Mode {
    static SELECTED: OnceLock<Mode> = OnceLock::new();
    *SELECTED.get_or_init(|| {
        match env::var("MEM_BPB3K_KERNEL").as_deref() {
            Ok("simd") => Mode::Simd,
            Ok("tiled") => Mode::Tiled,
            Ok("hybrid") => Mode::Hybrid,
            Ok("fused") => Mode::Fused,
            _ => Mode::Scalar,
        }
    })
}
fn u64_at(block: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(block[p..p+8].try_into().expect("fixed BPB3-K payload"))
}
fn coeff(block: &[u8], i: usize) -> i32 {
    let off = (i/64)*24;
    let b = i%64;
    let sign = (u64_at(block,off)>>b)&1;
    let lo = (u64_at(block,off+8)>>b)&1;
    let hi = (u64_at(block,off+16)>>b)&1;
    let mag = (lo+2*hi) as i32;
    if sign!=0 { -mag-1 } else { mag }
}
fn scale(block: &[u8], g: usize) -> i32 {
    let sc=&block[96..108];
    let low=if g<8 {sc[g]&15} else {sc[g-8]>>4};
    let high=(sc[8+g%4]>>(2*(g/4)))&3;
    i32::from(low|(high<<4))-32
}
fn fp16(v: u16) -> f32 {
    // Exact IEEE 754 FP16-to-FP32 widening for normals, zero and infinity:
    // avoid per-superblock powi/libm calls in the token-generation hot path.
    let sign=((v as u32)&0x8000)<<16;
    let exponent=((v>>10)&31) as u32;
    let fraction=(v as u32)&1023;
    if exponent==0 {
        if fraction==0 {return f32::from_bits(sign);}
        let magnitude=(fraction as f32)*(1.0f32/16_777_216.0f32);
        return if sign!=0 {-magnitude} else {magnitude};
    }
    if exponent==31 {
        return f32::from_bits(sign|0x7f80_0000|(fraction<<13));
    }
    f32::from_bits(sign|((exponent+112)<<23)|(fraction<<13))
}

fn global_scale(block: &[u8], act: &[u8]) -> f32 {
    fp16(u16::from_le_bytes([block[108],block[109]]))
        * f32::from_le_bytes(act[..4].try_into().unwrap())
}
fn finish(sums: [f32; LANES]) -> f32 {
    let mut result=0.0f32;
    for x in sums { result+=x; }
    result
}

fn reference(weights: &[u8], activations: &[u8]) -> f32 {
    let mut sums=[0.0f32;LANES];
    for (block,act) in weights.chunks_exact(BLOCK_W).zip(activations.chunks_exact(BLOCK_A)) {
        let mut ints=[0i32;LANES];
        for g in 0..16 {
            let sc=scale(block,g);
            for lane in 0..8 {
                let a=16*g+lane;
                let b=a+8;
                let va=(act[4+a] as i8) as i32;
                let vb=(act[4+b] as i8) as i32;
                ints[lane]+=sc*(coeff(block,a)*va+coeff(block,b)*vb);
            }
        }
        let d=global_scale(block,act);
        for lane in 0..LANES { sums[lane]+=d*(ints[lane] as f32); }
    }
    finish(sums)
}

#[cfg(target_arch="x86_64")]
mod x86 {
    use super::*;
    use std::arch::x86_64::*;
    // Every AVX2 operation is isolated in this dynamically guarded function;
    // no global -march=native or AVX2 assumption in the main code.
    // Fused AVX2: keep eight integer accumulators in registers over the
    // entire 256-element superblock; loads 3 bitplane words per 16-element
    // subgroup rather than per 8-element half. One store per superblock.
    #[target_feature(enable="avx2")]
    pub unsafe fn fused_block(w:&[u8],a:&[u8])->[i32;8] {
        let bitmask=_mm256_setr_epi32(1,2,4,8,16,32,64,128);
        let one=_mm256_set1_epi32(1);
        let two=_mm256_set1_epi32(2);
        let zero=_mm256_setzero_si256();
        let mut sum=zero;
        for g in 0..16usize {
            let off=(g/4)*24;
            let shift=(g%4)*16;
            let neg=u64_at(w,off)>>shift;
            let lo=u64_at(w,off+8)>>shift;
            let hi=u64_at(w,off+16)>>shift;
            let sc=_mm256_set1_epi32(scale(w,g));
            for half in 0..2usize {
                let bitshift=half*8;
                let negbits=_mm256_set1_epi32(((neg>>bitshift)&255) as i32);
                let lobits=_mm256_set1_epi32(((lo>>bitshift)&255) as i32);
                let hibits=_mm256_set1_epi32(((hi>>bitshift)&255) as i32);
                let negmask=_mm256_cmpeq_epi32(_mm256_and_si256(bitmask,negbits),bitmask);
                let low=_mm256_and_si256(_mm256_cmpeq_epi32(_mm256_and_si256(bitmask,lobits),bitmask),one);
                let high=_mm256_and_si256(_mm256_cmpeq_epi32(_mm256_and_si256(bitmask,hibits),bitmask),two);
                let magnitude=_mm256_add_epi32(low,high);
                let signed=_mm256_blendv_epi8(magnitude,_mm256_sub_epi32(zero,_mm256_add_epi32(magnitude,one)),negmask);
                let input=_mm_loadl_epi64(a.as_ptr().add(4+g*16+half*8) as *const __m128i);
                let q8=_mm256_cvtepi8_epi32(input);
                sum=_mm256_add_epi32(sum,_mm256_mullo_epi32(sc,_mm256_mullo_epi32(signed,q8)));
            }
        }
        let mut out=[0i32;8];
        _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i,sum);
        out
    }
    #[target_feature(enable="avx2")]
    pub unsafe fn product8(block:&[u8],act:&[u8], start:usize) -> [i32;8] {
        let group=start/64;
        let lane=start%64;
        let off=group*24;
        let neg=(u64_at(block,off)>>lane) as i32&255;
        let lo=(u64_at(block,off+8)>>lane) as i32&255;
        let hi=(u64_at(block,off+16)>>lane) as i32&255;
        let bits=_mm256_setr_epi32(1,2,4,8,16,32,64,128);
        let lo_v=_mm256_and_si256(_mm256_cmpeq_epi32(_mm256_and_si256(bits,_mm256_set1_epi32(lo)),bits),_mm256_set1_epi32(1));
        let hi_v=_mm256_and_si256(_mm256_cmpeq_epi32(_mm256_and_si256(bits,_mm256_set1_epi32(hi)),bits),_mm256_set1_epi32(2));
        let mag=_mm256_add_epi32(lo_v,hi_v);
        let signed=_mm256_blendv_epi8(mag,_mm256_sub_epi32(_mm256_setzero_si256(),_mm256_add_epi32(mag,_mm256_set1_epi32(1))),_mm256_cmpeq_epi32(_mm256_and_si256(bits,_mm256_set1_epi32(neg)),bits));
        // Unaligned eight signed activation bytes -> eight 32-bit signed lanes.
        let bytes=_mm_loadl_epi64(act.as_ptr().add(4+start) as *const __m128i);
        let q8=_mm256_cvtepi8_epi32(bytes);
        let product=_mm256_mullo_epi32(signed,q8);
        let mut result=[0i32;8];
        _mm256_storeu_si256(result.as_mut_ptr() as *mut __m256i,product);
        result
    }
}
fn avx2_available() -> bool {
    #[cfg(target_arch="x86_64")]
    { std::is_x86_feature_detected!("avx2") }
    #[cfg(not(target_arch="x86_64"))]
    { false }
}
fn lane_products(block:&[u8],act:&[u8],start:usize,simd:bool)->[i32;8] {
    if simd && avx2_available() {
        #[cfg(target_arch="x86_64")]
        { return unsafe { x86::product8(block,act,start) }; }
    }
    std::array::from_fn(|l|coeff(block,start+l)*(act[4+start+l] as i8 as i32))
}
fn grouped(weights:&[&[u8]],act:&[u8],simd:bool)->Vec<f32> {
    let rows=weights.len();
    assert!((1..=TILE).contains(&rows));
    let blocks=act.len()/BLOCK_A;
    assert_eq!(act.len()%BLOCK_A,0);
    for w in weights { assert_eq!(w.len(),blocks*BLOCK_W); }
    let mut sums=[[0.0f32;LANES];TILE];
    for b in 0..blocks {
        let a=&act[b*BLOCK_A..(b+1)*BLOCK_A];
        let mut acc=[[0i32;LANES];TILE];
        for g in 0..16 {
            for half in 0..2 {
                let start=16*g+8*half;
                for row in 0..rows {
                    let w=&weights[row][b*BLOCK_W..(b+1)*BLOCK_W];
                    let v=lane_products(w,a,start,simd);
                    let s=scale(w,g);
                    for lane in 0..LANES { acc[row][lane]+=v[lane]*s; }
                }
            }
        }
        for row in 0..rows {
            let w=&weights[row][b*BLOCK_W..(b+1)*BLOCK_W];
            let d=global_scale(w,a);
            for lane in 0..LANES { sums[row][lane]+=d*(acc[row][lane] as f32); }
        }
    }
    sums[..rows].iter().copied().map(finish).collect()
}

// Fused AVX2 experiment maintains exactly the eight ggml integer lanes and
// f32 reductions; the existing modes remain independent controls.
fn fused_rows(weights:&[&[u8]],act:&[u8])->Vec<f32> {
    let rows=weights.len();
    assert!((1..=TILE).contains(&rows));
    assert_eq!(act.len()%BLOCK_A,0);
    let blocks=act.len()/BLOCK_A;
    for w in weights {assert_eq!(w.len(),blocks*BLOCK_W);}
    if !avx2_available() {return weights.iter().map(|w|reference(w,act)).collect();}
    let mut sums=[[0.0f32;LANES];TILE];
    for b in 0..blocks {
        let input=&act[b*BLOCK_A..(b+1)*BLOCK_A];
        for row in 0..rows {
            let w=&weights[row][b*BLOCK_W..(b+1)*BLOCK_W];
            #[cfg(target_arch="x86_64")]
            let ints=unsafe {x86::fused_block(w,input)};
            #[cfg(not(target_arch="x86_64"))]
            let ints={
                let mut lanes=[0i32;LANES];
                for g in 0..16 {
                    let sc=scale(w,g);
                    for l in 0..LANES {
                        let x=g*16+l;
                        lanes[l]+=sc*(coeff(w,x)*(input[4+x] as i8 as i32)
                                     +coeff(w,x+8)*(input[4+x+8] as i8 as i32));
                    }
                }
                lanes
            };
            let d=global_scale(w,input);
            for lane in 0..LANES {sums[row][lane]+=d*(ints[lane] as f32);}
        }
    }
    sums[..rows].iter().copied().map(finish).collect()
}
/// Native ggml vec_dot ABI. bx/by are byte strides; bs is an f32-element
/// stride (matching ggml's multi-row kernel convention). Upstream tensors
/// determine buffer bounds; do not expose this entry to untrusted callers.
///
/// # Safety
/// vx must reference nrc rows of n/256 contiguous 110-byte BPB3-K blocks with
/// bx bytes between rows, and vy must reference a Q8_K row of n/256*292 bytes.
/// s must be writable at the nrc output positions separated by bs f32s.
#[no_mangle]
pub unsafe extern "C" fn mem_bpb3k_dot_q8(
    n:i32, s:*mut f32, bs:usize, vx:*const c_void, bx:usize,
    vy:*const c_void, by:usize, nrc:i32,
) {
    assert!(n>0 && n%256==0 && (1..=4).contains(&nrc));
    let bytes=n as usize/256;
    // ggml nrc is a square microtile: 4 adjacent weight rows and
    // 4 adjacent activation columns; s[activation * bs + weight].
    let nrc = nrc as usize;
    let rows:Vec<_>=(0..nrc).map(|i| {
        slice::from_raw_parts((vx as *const u8).add(i*bx),bytes*BLOCK_W)
    }).collect();
    let m=mode();
    static TRACE: OnceLock<()> = OnceLock::new();
    TRACE.get_or_init(|| eprintln!("BPB3K_DIRECT_DOT_EXECUTED kernel={m:?} rows={nrc} avx2={}",avx2_available()));

    for col in 0..nrc {
        let act=slice::from_raw_parts((vy as *const u8).add(col*by),bytes*BLOCK_A);
        let values:Vec<f32>=match m {
            Mode::Scalar => rows.iter().map(|r|reference(r,act)).collect(),
            Mode::Simd => rows.iter().map(|r|grouped(&[*r],act,true)[0]).collect(),
            Mode::Tiled => grouped(&rows,act,false),
            Mode::Hybrid => grouped(&rows,act,true),
            Mode::Fused => fused_rows(&rows,act),
        };
        for (row,value) in values.iter().enumerate() {
            *s.add(col*bs+row)=*value;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn wire(seed:usize, blocks:usize)->Vec<u8> {
        let mut data=vec![0u8;blocks*BLOCK_W];
        for (i,x) in data.iter_mut().enumerate() {*x=((i*37+i/3*13+seed*71)%256) as u8;}
        for b in 0..blocks {data[b*BLOCK_W+108..b*BLOCK_W+110].copy_from_slice(&0x3c00u16.to_le_bytes());}
        data
    }
    fn input(seed:usize,blocks:usize)->Vec<u8> {
        let mut data=vec![0u8;blocks*BLOCK_A];
        for b in 0..blocks {
            data[b*BLOCK_A..b*BLOCK_A+4].copy_from_slice(&(1.0f32+(seed as f32)*0.125).to_le_bytes());
            for i in 0..256 {data[b*BLOCK_A+4+i]=((i*17+b*31+seed*47)%256) as u8;}
        }
        data
    }
    #[test] fn kernel_four_way_parity_real_layout() {
        for blocks in [1,2,3,7] {
            let ws=(0..4).map(|r|wire(r,blocks)).collect::<Vec<_>>();
            let a=input(3,blocks);
            let refs=ws.iter().map(|w|reference(w,&a)).collect::<Vec<_>>();
            let slices=ws.iter().map(|w|w.as_slice()).collect::<Vec<_>>();
            for mode in [false,true] {
                let actual=grouped(&slices,&a,mode);
                for (i,(x,y)) in actual.iter().zip(refs.iter()).enumerate() {
                    assert_eq!(x.to_bits(),y.to_bits(),"mode={mode} blocks={blocks} row={i}");
                }
            }
        }
    }
    #[test] fn fused_avx2_superblock_exact_eight_lane_oracle() {
        for blocks in [1,2,3,7,31] {
            let ws=(0..4).map(|r|wire(r+11,blocks)).collect::<Vec<_>>();
            let act=input(17,blocks);
            let slices=ws.iter().map(|w|w.as_slice()).collect::<Vec<_>>();
            let actual=fused_rows(&slices,&act);
            for (row,w) in ws.iter().enumerate() {
                let expected=reference(w,&act);
                assert_eq!(actual[row].to_bits(),expected.to_bits(),"row={row} blocks={blocks}");
            }
        }
    }
    #[test] fn ggml_runtime_nrc2_uses_safe_two_by_two_scratch_layout() {
        let blocks=4usize;
        let weights=(0..2).map(|row|wire(row+7,blocks)).collect::<Vec<_>>();
        let activations=(0..2).map(|col|input(col+2,blocks)).collect::<Vec<_>>();
        let mut wb=Vec::new();
        let mut ab=Vec::new();
        for row in &weights {wb.extend_from_slice(row);}
        for col in &activations {ab.extend_from_slice(col);}
        let mut output=[f32::NAN;32]; // upstream ggml uses exactly tmp[32]
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks*256) as i32,output.as_mut_ptr(),16,
                wb.as_ptr() as *const c_void, blocks*BLOCK_W,
                ab.as_ptr() as *const c_void,blocks*BLOCK_A,2,
            );
        }
        for col in 0..2 {
            for row in 0..2 {
                let expected=reference(&weights[row],&activations[col]);
                assert_eq!(output[col*16+row].to_bits(),expected.to_bits(),"col={col} row={row}");
            }
        }
        assert!(output[2..16].iter().all(|x|x.is_nan()));
        assert!(output[18..32].iter().all(|x|x.is_nan()));
    }
    #[test] fn ggml_square_nrc4_abi_matches_16_reference_outputs() {
        let blocks=3usize;
        let ws=(0..4).map(|row|wire(row,blocks)).collect::<Vec<_>>();
        let acts=(0..4).map(|col|input(col,blocks)).collect::<Vec<_>>();
        let mut output=[f32::NAN;64];
        let old=std::env::var_os("MEM_BPB3K_KERNEL");
        // Standalone tests process has no env override by default.
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks*256) as i32,output.as_mut_ptr(),16,
                ws[0].as_ptr() as *const c_void,
                0,acts[0].as_ptr() as *const c_void,0,1,
            );
        }
        // Actual four-stride ABI: one contiguous buffer per tensor family.
        let mut wb=Vec::new();
        for w in &ws { wb.extend_from_slice(w); }
        let mut ab=Vec::new();
        for a in &acts { ab.extend_from_slice(a); }
        let mut got=[f32::NAN;64];
        unsafe {
            mem_bpb3k_dot_q8(
                (blocks*256) as i32,got.as_mut_ptr(),16,
                wb.as_ptr() as *const c_void, blocks*BLOCK_W,
                ab.as_ptr() as *const c_void,blocks*BLOCK_A,4,
            );
        }
        for col in 0..4 {
            for row in 0..4 {
                let expected=reference(&ws[row],&acts[col]);
                assert_eq!(got[col*16+row].to_bits(),expected.to_bits(),"col={col} row={row}");
            }
        }
        for col in 0..4 { for row in 4..16 { assert!(got[col*16+row].is_nan()); } }
        assert!(output[0].is_finite());
        drop(old);
    }
    #[test] fn fp16_exact_widening_exhaustive_all_finite_values() {
        for bits in 0u16..=u16::MAX {
            let exponent=(bits>>10)&31;
            if exponent==31 {continue;}
            let sign=if bits&0x8000!=0 {-1.0f32} else {1.0f32};
            let mantissa=(bits&1023) as f32;
            let expected=if exponent==0 {
                sign*mantissa*2.0f32.powi(-24)
            } else {
                sign*(1.0+mantissa/1024.0)*2.0f32.powi(i32::from(exponent)-15)
            };
            assert_eq!(fp16(bits).to_bits(),expected.to_bits(),"fp16={bits:04x}");
        }
    }
    #[test] fn coefficient_decode_signed_range() {
        let w=wire(5,1);
        for i in 0..256 {assert!((-4..=3).contains(&coeff(&w,i)));}
    }
}
