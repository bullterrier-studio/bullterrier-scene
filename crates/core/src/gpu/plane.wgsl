// A layer landed on the output (straight RGBA) as a mask's values: the alpha, or the Rec. 709 luminance times the alpha, times
// the layer's opacity and, if the layer has masks of its own, their product (`aux`, a plane): `CanvasMask::drawn`.
//   ints: 0 columns, 1 rows, 2 use (0 alpha, 1 luminance), 3 has a factor    reals: 0 opacity
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<f32>;
@group(0) @binding(3) var<storage, read> factor: array<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    let q = src[index];
    var value = q.w;
    if int_at(2u) == 1u {
        value = (q.x * 0.2126 + q.y * 0.7152 + q.z * 0.0722) * q.w;
    }
    value = value * real_at(0u);
    if int_at(3u) == 1u {
        value = value * factor[index];
    }
    dest[index] = value;
}
