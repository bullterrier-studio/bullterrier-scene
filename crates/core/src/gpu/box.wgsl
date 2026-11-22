// One box pass of a feather (`feather.rs`, `Lines::once`): the mean of 2n + 1 values with the two beyond them at weight f,
// the line's ends repeated past it.
//   ints: 0 columns, 1 rows, 2 axis (0 along x, 1 along y), 3 n    reals: 0 f
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dest: array<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let along_y = int_at(2u) == 1u;
    let n = i32(int_at(3u));
    let f = real_at(0u);
    var length = i32(out_columns());
    var position = i32(id.x);
    if along_y {
        length = i32(out_rows());
        position = i32(id.y);
    }
    var sum = 0.0;
    for (var k = -n - 1; k <= n + 1; k++) {
        let at = clamp(position + k, 0, length - 1);
        var value = 0.0;
        if along_y {
            value = src[u32(at) * out_columns() + id.x];
        } else {
            value = src[id.y * out_columns() + u32(at)];
        }
        if k == -n - 1 || k == n + 1 {
            sum += f * value;
        } else {
            sum += value;
        }
    }
    dest[id.y * out_columns() + id.x] = sum / (f32(2 * n + 1) + 2.0 * f);
}
