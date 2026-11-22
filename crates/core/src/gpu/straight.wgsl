// The join (premultiplied) as straight alpha: the colour over the alpha, nothing where nothing is drawn (`render::straight`).
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    let q = src[index];
    var colour = vec3<f32>(0.0);
    if q.w > 0.0 {
        colour = q.xyz / q.w;
    }
    dest[index] = vec4<f32>(colour, q.w);
}
