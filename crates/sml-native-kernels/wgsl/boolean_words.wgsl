// SML-GROK-NATIVE-A2: vendor-neutral twin of ptx/boolean_words.ptx.
// One invocation per u32 lane; bounds-guarded.
@group(0) @binding(0) var<storage, read> a: array<u32>;
@group(0) @binding(1) var<storage, read> b: array<u32>;
@group(0) @binding(2) var<storage, read> c: array<u32>;
@group(0) @binding(3) var<storage, read_write> o: array<u32>;

@compute @workgroup_size(64)
fn sml_xor3_u32(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&o)) { o[i] = a[i] ^ b[i] ^ c[i]; }
}

@compute @workgroup_size(64)
fn sml_hamming_u32(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&o)) { o[i] = countOneBits(a[i] ^ b[i]); }
}

@compute @workgroup_size(64)
fn sml_ternary_partial_u32(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&o)) {
        let p = i32(countOneBits(a[i] & b[i]));
        let m = i32(countOneBits(a[i] & c[i]));
        o[i] = bitcast<u32>(p - m);
    }
}
