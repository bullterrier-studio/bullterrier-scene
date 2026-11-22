// `reduce` of `warp.rs`, first along each row: the premultiplied mean of the source pixels each output column covers.
//   ints: 0 out columns (the reduced width), 1 rows (the source's), 2 source columns
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let source_len = int_at(2u);
    let step = f32(source_len) / f32(out_columns());
    let start = f32(id.x) * step;
    let end = f32(id.x + 1u) * step;
    let last = min(u32(ceil(end)), source_len);
    var sum = vec4<f32>(0.0);
    for (var at = u32(floor(start)); at < last; at++) {
        let weight = (min(end, f32(at + 1u)) - max(start, f32(at))) / step;
        let q = src[id.y * source_len + at];
        sum += weight * vec4<f32>(q.rgb * q.a, q.a);
    }
    dest[id.y * out_columns() + id.x] = sum;
}
