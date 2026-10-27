//! Experimental low-level Boolean kernels for SML-GENIUS.
//!
//! This crate is intentionally isolated from the reference model and inference
//! runtime. CPU assembly requires explicit, audited unsafe blocks. GPU PTX
//! requires an opt-in driver adapter and a real device for performance claims.
//! Never select a backend solely because assembly *looks* faster.

#![deny(unsafe_op_in_unsafe_fn)]

/// Verbatim PTX text, JIT-compiled by the optional CUDA *driver* adapter.
/// PTX is a NVIDIA virtual ISA, not hardware SASS or a measured performance win.
pub const NVIDIA_PTX: &str = include_str!("../ptx/boolean_words.ptx");

/// CPU execution path chosen once per kernel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuPath {
    /// Portable, optimizer-friendly bit operations.
    Portable,
    /// Hardware POPCNT available at runtime.
    #[cfg(target_arch = "x86_64")]
    X86Popcnt,
    /// NEON byte-count and horizontal reduction available at runtime.
    #[cfg(target_arch = "aarch64")]
    Aarch64NeonCnt,
}

/// CPU Boolean word primitive dispatch; does not mutate global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuKernels {
    path: CpuPath,
}

impl Default for CpuKernels {
    fn default() -> Self {
        Self::detect()
    }
}

impl CpuKernels {
    /// Detect a supported CPU instruction set without assuming its presence.
    #[must_use]
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("popcnt") {
            return Self {
                path: CpuPath::X86Popcnt,
            };
        }
        #[cfg(target_arch = "aarch64")]
        if std::arch::is_aarch64_feature_detected!("neon") {
            return Self {
                path: CpuPath::Aarch64NeonCnt,
            };
        }
        Self {
            path: CpuPath::Portable,
        }
    }

    /// Force reference behavior to obtain a matched baseline.
    #[must_use]
    pub const fn portable() -> Self {
        Self {
            path: CpuPath::Portable,
        }
    }

    /// Exact backend identity for reports.
    #[must_use]
    pub const fn path(self) -> CpuPath {
        self.path
    }

    /// Population count of a 64-bit word.
    #[must_use]
    pub fn popcount(self, word: u64) -> u32 {
        match self.path {
            CpuPath::Portable => word.count_ones(),
            #[cfg(target_arch = "x86_64")]
            CpuPath::X86Popcnt => x86_popcnt(word),
            #[cfg(target_arch = "aarch64")]
            CpuPath::Aarch64NeonCnt => aarch64_neon_popcnt(word),
        }
    }

    /// GF(2) bitwise-dot-product parity.
    #[must_use]
    pub fn gf2_dot_parity(self, left: u64, right: u64) -> bool {
        self.popcount(left & right) & 1 != 0
    }

    /// Hamming distance between two 64-bit packed SBG words.
    #[must_use]
    pub fn hamming(self, left: u64, right: u64) -> u32 {
        self.popcount(left ^ right)
    }

    /// One signed ternary dot partial with disjoint plus/minus weight masks.
    /// Both counts lie in [0,64], so the signed difference is exact in i32.
    #[must_use]
    pub fn ternary_partial(self, input: u64, plus: u64, minus: u64) -> i32 {
        self.popcount(input & plus) as i32 - self.popcount(input & minus) as i32
    }

    /// Batched Hamming distance with no allocation. Reject shape mismatch
    /// before writing any element, preserving output on an invalid request.
    pub fn hamming_into(
        self,
        left: &[u64],
        right: &[u64],
        out: &mut [u32],
    ) -> Result<(), LengthMismatch> {
        if left.len() != right.len() || left.len() != out.len() {
            return Err(LengthMismatch {
                left: left.len(),
                right: right.len(),
                output: out.len(),
            });
        }
        for ((&a, &b), slot) in left.iter().zip(right).zip(out.iter_mut()) {
            *slot = self.hamming(a, b);
        }
        Ok(())
    }
}

/// Shape error detected before a batched write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LengthMismatch {
    /// First input length, in 64-bit words.
    pub left: usize,
    /// Second input length, in 64-bit words.
    pub right: usize,
    /// Output length, in elements.
    pub output: usize,
}

#[cfg(target_arch = "x86_64")]
#[inline]
fn x86_popcnt(word: u64) -> u32 {
    let bits: u64;
    // SAFETY: x86 POPCNT reads the 64-bit register operand, writes a 64-bit
    // output register, accesses no memory/stack, and is invoked only after
    // is_x86_feature_detected!("popcnt") checks the host CPU.
    unsafe {
        core::arch::asm!(
            "popcnt {output}, {input}",
            output = lateout(reg) bits,
            input = in(reg) word,
            options(pure, nomem, nostack),
        );
    }
    bits as u32
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn aarch64_neon_popcnt(word: u64) -> u32 {
    let bits: u32;
    // SAFETY: NEON is checked before this function is selected. v0 is
    // explicitly clobbered; the code reads one GPR input and writes one
    // GPR output, using CNT+ADDV+UMOV without touching memory or the stack.
    unsafe {
        core::arch::asm!(
            "fmov d0, {input}",
            "cnt v0.8b, v0.8b",
            "addv b0, v0.8b",
            "umov {output:w}, v0.b[0]",
            input = in(reg) word,
            output = lateout(reg) bits,
            out("v0") _,
            options(pure, nomem, nostack),
        );
    }
    bits
}

/// Each PTX scalar instruction's CPU semantic contract. The on-device result
/// still requires a real GPU comparison (never inferred from these tests).
#[must_use]
pub fn ptx_word_oracle(operation: PtxOperation, a: u32, b: u32, c: u32) -> u32 {
    match operation {
        PtxOperation::Xor3 => a ^ b ^ c,
        PtxOperation::Hamming => (a ^ b).count_ones(),
        PtxOperation::TernaryPartial => {
            ((a & b).count_ones() as i32 - (a & c).count_ones() as i32) as u32
        }
    }
}

/// Supported PTX word operations: same three inputs and one output layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtxOperation {
    /// Three-way GF(2) XOR using LOP3 LUT 0x96.
    Xor3,
    /// Hamming distance using XOR + POPC.
    Hamming,
    /// Per-word binary activation × ternary weights using AND + POPC.
    TernaryPartial,
}

impl PtxOperation {
    /// PTX export symbol selected by the optional NVIDIA CUDA driver adapter.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Xor3 => "sml_xor3_u32",
            Self::Hamming => "sml_hamming_u32",
            Self::TernaryPartial => "sml_ternary_partial_u32",
        }
    }
}

#[cfg(all(target_os = "linux", feature = "cuda-driver"))]
pub mod cuda_driver;

#[cfg(feature = "portable-gpu")]
pub mod portable_gpu;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_assembly_vs_reference_on_adversarial_words() {
        let active = CpuKernels::detect();
        let reference = CpuKernels::portable();
        let words = [
            0,
            u64::MAX,
            1,
            1 << 63,
            0x5555_5555_5555_5555,
            0xaaaa_aaaa_aaaa_aaaa,
            0x0123_4567_89ab_cdef,
            0xffff_0000_ffff_0000,
        ];
        for a in words {
            assert_eq!(active.popcount(a), reference.popcount(a));
            for b in words {
                assert_eq!(active.hamming(a, b), reference.hamming(a, b));
                assert_eq!(active.gf2_dot_parity(a, b), reference.gf2_dot_parity(a, b));
                for c in words {
                    assert_eq!(
                        active.ternary_partial(a, b, c),
                        reference.ternary_partial(a, b, c)
                    );
                }
            }
        }
    }

    #[test]
    fn extensive_pseudorandom_bits_against_native_rust_oracle() {
        let active = CpuKernels::detect();
        let mut x = 0x91e1_0da5_98b6_47aa_u64;
        for _ in 0..65_536 {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            let a = x;
            let b = x.rotate_left(19) ^ 0x9e37_79b9_7f4a_7c15;
            let c = x.rotate_right(7) ^ u64::MAX;
            assert_eq!(active.popcount(a), a.count_ones());
            assert_eq!(active.gf2_dot_parity(a, b), ((a & b).count_ones() & 1) != 0);
            assert_eq!(active.hamming(a, b), (a ^ b).count_ones());
            assert_eq!(
                active.ternary_partial(a, b, c),
                (a & b).count_ones() as i32 - (a & c).count_ones() as i32
            );
            assert_eq!(
                ptx_word_oracle(PtxOperation::Xor3, a as u32, b as u32, c as u32),
                (a ^ b ^ c) as u32
            );
        }
    }

    #[test]
    fn batch_length_mismatch_is_atomic() {
        let k = CpuKernels::detect();
        let mut out = [999_u32; 2];
        let invalid = k.hamming_into(&[0, 1], &[0], &mut out);
        assert_eq!(
            invalid,
            Err(LengthMismatch {
                left: 2,
                right: 1,
                output: 2
            })
        );
        assert_eq!(out, [999, 999]);
        k.hamming_into(&[0, u64::MAX], &[0, 0], &mut out).unwrap();
        assert_eq!(out, [0, 64]);
    }

    #[test]
    fn exact_ptx_lop3_truth_table_xor3() {
        // PTX LOP3: index = a | (b << 1) | (c << 2).
        // Three-input XOR is symmetric, so its LUT is 0x96 in either order.
        for a in 0_u32..2 {
            for b in 0_u32..2 {
                for c in 0_u32..2 {
                    let index = a | (b << 1) | (c << 2);
                    let lut = (0x96_u32 >> index) & 1;
                    assert_eq!(lut, a ^ b ^ c);
                }
            }
        }
        assert!(NVIDIA_PTX.contains("lop3.b32"));
        assert!(NVIDIA_PTX.contains("0x96"));
        assert!(NVIDIA_PTX.contains("popc.b32"));
    }

    #[test]
    fn ptx_scalar_oracles_cover_masks_and_signs() {
        for a in 0_u32..256 {
            for b in 0_u32..256 {
                let c = (!b) & 255;
                assert_eq!(
                    ptx_word_oracle(PtxOperation::Hamming, a, b, c),
                    (a ^ b).count_ones()
                );
                let signed = (a & b).count_ones() as i32 - (a & c).count_ones() as i32;
                assert_eq!(
                    ptx_word_oracle(PtxOperation::TernaryPartial, a, b, c) as i32,
                    signed
                );
            }
        }
    }
}
