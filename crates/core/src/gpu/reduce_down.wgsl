// `reduce` of `warp.rs`, then down each column, and straight again.
//   ints: 0 columns, 1 out rows (the reduced height), 2 source rows
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let source_len = int_at(2u);
    let step = f32(source_len) / f32(out_rows());
    let start = f32(id.y) * step;
    let end = f32(id.y + 1u) * step;
    let last = min(u32(ceil(end)), source_len);
    var sum = vec4<f32>(0.0);
    for (var at = u32(floor(start)); at < last; at++) {
        let weight = (min(end, f32(at + 1u)) - max(start, f32(at))) / step;
        sum += weight * src[at * out_columns() + id.x];
    }
    var colour = vec3<f32>(0.0);
    if sum.w > 0.0 {
        colour = sum.xyz / sum.w;
    }
    dest[id.y * out_columns() + id.x] = vec4<f32>(colour, min(sum.w, 1.0));
}
