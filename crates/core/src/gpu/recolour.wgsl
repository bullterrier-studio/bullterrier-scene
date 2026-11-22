// A look's picture (`aux`, RGB the effects made, from image's executor) on the layer's own alpha (`src`): a look keeps the alpha it
// was given and its colours are brought into 0 to 1 (`Look::apply`).
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> looked: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    dest[index] = vec4<f32>(clamp(looked[index].xyz, vec3<f32>(0.0), vec3<f32>(1.0)), src[index].w);
}
