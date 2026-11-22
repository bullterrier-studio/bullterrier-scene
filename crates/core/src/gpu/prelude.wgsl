// Shared by every kernel: the parameter block (`ints` and `reals`, written by `Call`), the output size every kernel starts
// from, and the constants. A kernel declares the storage bindings it uses (1 `src`, 2 `dst`, 3 `aux`, 4 `extra`) itself.
// `target` is a reserved word in WGSL: kernels say `dest`.
struct Params {
    ints: array<vec4<u32>, 4>,
    reals: array<vec4<f32>, 12>,
}

@group(0) @binding(0) var<uniform> p: Params;

fn int_at(i: u32) -> u32 {
    return p.ints[i / 4u][i % 4u];
}

fn real_at(i: u32) -> f32 {
    return p.reals[i / 4u][i % 4u];
}

// The output of every kernel is `int_at(0) x int_at(1)` (columns x rows), one invocation a pixel.
fn out_columns() -> u32 {
    return int_at(0u);
}

fn out_rows() -> u32 {
    return int_at(1u);
}
