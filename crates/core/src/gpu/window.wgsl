// A rectangle of a plane of f32 copied out, an index that falls outside the input clamped to its edge: a plane padded by the
// edge's value (offset negative), or cropped (offset positive).
//   ints: 0 out columns, 1 out rows, 2 in columns, 3 in rows, 4 and 5 the offset of the output's origin in the input (x, y), as i32
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dest: array<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let columns = i32(int_at(2u));
    let rows = i32(int_at(3u));
    let x = clamp(i32(id.x) + bitcast<i32>(int_at(4u)), 0, columns - 1);
    let y = clamp(i32(id.y) + bitcast<i32>(int_at(5u)), 0, rows - 1);
    dest[id.y * out_columns() + id.x] = src[u32(y * columns + x)];
}
