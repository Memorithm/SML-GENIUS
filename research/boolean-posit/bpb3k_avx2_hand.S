# x86-64 SysV ABI: int32_t mem_bpb3k_dot_avx2_asm(const uint8_t *weights,
#                                               const uint8_t *signed_q8_block);
# Called only after exact-length input and AVX2 CPU feature checks in Rust.
# BPB3-K weights: 110 bytes (four 3*8-byte bitplane groups; 12 scale bytes).
# Q8_K block:    292 bytes (float scale, 256 signed codes, 32 cached sums).
# No stack frame, stack loads/stores, callee-save GPR writes, heap, or alignment
# requirements on either input. Only read-only aligned constants are vmovdqa.
# Full-source/preprocessed assembly and emitted binary are diagnostics only.
.intel_syntax noprefix
.text
.p2align 5
.globl mem_bpb3k_dot_avx2_asm
.type mem_bpb3k_dot_avx2_asm, @function
mem_bpb3k_dot_avx2_asm:
    vmovdqa ymm14, YMMWORD PTR [rip + .Lshuffle]
    vmovdqa ymm13, YMMWORD PTR [rip + .Lbits]
    vmovdqa ymm12, YMMWORD PTR [rip + .Lones]
    vmovdqa ymm11, YMMWORD PTR [rip + .Lfours]
    vpxor ymm15, ymm15, ymm15
    vpxor ymm8, ymm8, ymm8
    # group 0 pair 0: original signed weight indices 0..31
    mov eax, DWORD PTR [rdi + 0]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 8]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 16]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 4]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 96]
    and eax, 15
    movzx edx, BYTE PTR [rdi + 104]
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 97]
    and ecx, 15
    movzx edx, BYTE PTR [rdi + 105]
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 0 pair 1: original signed weight indices 32..63
    mov eax, DWORD PTR [rdi + 4]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 12]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 20]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 36]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 98]
    and eax, 15
    movzx edx, BYTE PTR [rdi + 106]
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 99]
    and ecx, 15
    movzx edx, BYTE PTR [rdi + 107]
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 1 pair 0: original signed weight indices 64..95
    mov eax, DWORD PTR [rdi + 24]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 32]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 40]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 68]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 100]
    and eax, 15
    movzx edx, BYTE PTR [rdi + 104]
    shr edx, 2
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 101]
    and ecx, 15
    movzx edx, BYTE PTR [rdi + 105]
    shr edx, 2
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 1 pair 1: original signed weight indices 96..127
    mov eax, DWORD PTR [rdi + 28]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 36]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 44]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 100]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 102]
    and eax, 15
    movzx edx, BYTE PTR [rdi + 106]
    shr edx, 2
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 103]
    and ecx, 15
    movzx edx, BYTE PTR [rdi + 107]
    shr edx, 2
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 2 pair 0: original signed weight indices 128..159
    mov eax, DWORD PTR [rdi + 48]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 56]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 64]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 132]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 96]
    shr eax, 4
    movzx edx, BYTE PTR [rdi + 104]
    shr edx, 4
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 97]
    shr ecx, 4
    movzx edx, BYTE PTR [rdi + 105]
    shr edx, 4
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 2 pair 1: original signed weight indices 160..191
    mov eax, DWORD PTR [rdi + 52]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 60]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 68]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 164]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 98]
    shr eax, 4
    movzx edx, BYTE PTR [rdi + 106]
    shr edx, 4
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 99]
    shr ecx, 4
    movzx edx, BYTE PTR [rdi + 107]
    shr edx, 4
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 3 pair 0: original signed weight indices 192..223
    mov eax, DWORD PTR [rdi + 72]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 80]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 88]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 196]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 100]
    shr eax, 4
    movzx edx, BYTE PTR [rdi + 104]
    shr edx, 6
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 101]
    shr ecx, 4
    movzx edx, BYTE PTR [rdi + 105]
    shr edx, 6
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # group 3 pair 1: original signed weight indices 224..255
    mov eax, DWORD PTR [rdi + 76]
    vmovd xmm10, eax
    vpbroadcastd ymm0, xmm10
    vpshufb ymm0, ymm0, ymm14
    vpand ymm0, ymm0, ymm13
    vpcmpeqb ymm0, ymm0, ymm15
    vpandn ymm0, ymm0, ymm12
    mov eax, DWORD PTR [rdi + 84]
    vmovd xmm10, eax
    vpbroadcastd ymm1, xmm10
    vpshufb ymm1, ymm1, ymm14
    vpand ymm1, ymm1, ymm13
    vpcmpeqb ymm1, ymm1, ymm15
    vpandn ymm1, ymm1, ymm12
    mov eax, DWORD PTR [rdi + 92]
    vmovd xmm10, eax
    vpbroadcastd ymm2, xmm10
    vpshufb ymm2, ymm2, ymm14
    vpand ymm2, ymm2, ymm13
    vpcmpeqb ymm2, ymm2, ymm15
    vpandn ymm2, ymm2, ymm12
    # unsigned q+4 = (low XOR sign) + 2*(high XOR sign) + 4*(1-sign)
    vpxor ymm3, ymm1, ymm0
    vpxor ymm9, ymm2, ymm0
    vpsllw ymm9, ymm9, 1
    vpor ymm3, ymm3, ymm9
    vpxor ymm9, ymm0, ymm12
    vpsllw ymm9, ymm9, 2
    vpor ymm3, ymm3, ymm9
    # Each 2-byte sum is within signed 16-bit saturation limits.
    vmovdqu ymm4, YMMWORD PTR [rsi + 228]
    vpmaddubsw ymm5, ymm3, ymm4
    vpmaddubsw ymm6, ymm11, ymm4
    vpsubw ymm5, ymm5, ymm6
    movzx eax, BYTE PTR [rdi + 102]
    shr eax, 4
    movzx edx, BYTE PTR [rdi + 106]
    shr edx, 6
    and edx, 3
    shl edx, 4
    or eax, edx
    sub eax, 32
    movzx ecx, BYTE PTR [rdi + 103]
    shr ecx, 4
    movzx edx, BYTE PTR [rdi + 107]
    shr edx, 6
    and edx, 3
    shl edx, 4
    or ecx, edx
    sub ecx, 32
    # One signed 6-bit scale broadcast per 128-bit half, no spill.
    vmovd xmm10, eax
    vpbroadcastw ymm7, xmm10
    vmovd xmm10, ecx
    vpbroadcastw ymm9, xmm10
    vinserti128 ymm7, ymm7, xmm9, 1
    vpmaddwd ymm9, ymm5, ymm7
    vpaddd ymm8, ymm8, ymm9
    # Eight int32 lanes -> scalar, no variable-length stack storage.
    vextracti128 xmm1, ymm8, 1
    vpaddd xmm0, xmm8, xmm1
    vphaddd xmm0, xmm0, xmm0
    vphaddd xmm0, xmm0, xmm0
    vmovd eax, xmm0
    vzeroupper
    ret
.size mem_bpb3k_dot_avx2_asm, .-mem_bpb3k_dot_avx2_asm

.section .rodata
.p2align 5
.Lshuffle:
    .byte 0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1
    .byte 2,2,2,2,2,2,2,2,3,3,3,3,3,3,3,3
.p2align 5
.Lbits:
    .byte 1,2,4,8,16,32,64,128,1,2,4,8,16,32,64,128
    .byte 1,2,4,8,16,32,64,128,1,2,4,8,16,32,64,128
.p2align 5
.Lones:
    .rept 32
    .byte 1
    .endr
.p2align 5
.Lfours:
    .rept 32
    .byte 4
    .endr
.section .note.GNU-stack,"",@progbits
