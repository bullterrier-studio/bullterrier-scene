// Arithmetic on planes of f32.   ints: 0 columns, 1 rows, 2 operation (0 the product of two planes, 1 one minus a plane)
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dest: array<f32>;
@group(0) @binding(3) var<storage, read> other: array<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    if int_at(2u) == 0u {
        dest[index] = src[index] * other[index];
    } else {
        dest[index] = 1.0 - src[index];
    }
}
