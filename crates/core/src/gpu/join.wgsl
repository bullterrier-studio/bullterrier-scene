// A layer landed on the output (`src`, straight RGBA) joined over `dest` (premultiplied RGBA) at its opacity, times its masks'
// product (`aux`, a plane) where it has any: `render::lay` for one layer, with `blend::join` and `blend::lit` ported.
//   ints: 0 columns, 1 rows, 2 blend mode (the order of `Blend`), 3 flags (1 light, 2 has masks)    reals: 0 opacity
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> weights: array<f32>;

fn screen(cb: f32, cs: f32) -> f32 {
    return cb + cs - cb * cs;
}

fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb * 2.0 * cs;
    }
    return screen(cb, 2.0 * cs - 1.0);
}

fn separable(mode: u32, cb: f32, cs: f32) -> f32 {
    switch mode {
        case 1u: { return cb * cs; }
        case 2u: { return screen(cb, cs); }
        case 3u: { return hard_light(cs, cb); }
        case 4u: { return min(cb, cs); }
        case 5u: { return max(cb, cs); }
        case 6u: {
            if cb <= 0.0 { return 0.0; }
            if cs >= 1.0 { return 1.0; }
            return min(cb / (1.0 - cs), 1.0);
        }
        case 7u: {
            if cb >= 1.0 { return 1.0; }
            if cs <= 0.0 { return 0.0; }
            return 1.0 - min((1.0 - cb) / cs, 1.0);
        }
        case 8u: { return hard_light(cb, cs); }
        case 9u: {
            if cs <= 0.5 {
                return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
            }
            var d = sqrt(cb);
            if cb <= 0.25 {
                d = ((16.0 * cb - 12.0) * cb + 4.0) * cb;
            }
            return cb + (2.0 * cs - 1.0) * (d - cb);
        }
        case 10u: { return abs(cb - cs); }
        case 11u: { return cb + cs - 2.0 * cb * cs; }
        default: { return cs; }
    }
}

fn lum(c: vec3<f32>) -> f32 {
    return 0.3 * c.x + 0.59 * c.y + 0.11 * c.z;
}

fn clip_color(c: vec3<f32>, brightness: f32) -> vec3<f32> {
    let least = min(c.x, min(c.y, c.z));
    let most = max(c.x, max(c.y, c.z));
    var colour = c;
    if least < 0.0 {
        colour = brightness + (colour - brightness) * brightness / (brightness - least);
    }
    if most > 1.0 {
        colour = brightness + (colour - brightness) * (1.0 - brightness) / (most - brightness);
    }
    return colour;
}

fn set_lum(c: vec3<f32>, brightness: f32) -> vec3<f32> {
    let shift = brightness - lum(c);
    return clip_color(c + shift, brightness);
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.x, max(c.y, c.z)) - min(c.x, min(c.y, c.z));
}

fn set_sat(c: vec3<f32>, spread: f32) -> vec3<f32> {
    let least = min(c.x, min(c.y, c.z));
    let most = max(c.x, max(c.y, c.z));
    if most > least {
        return (c - least) * spread / (most - least);
    }
    return vec3<f32>(0.0);
}

fn mix_channels(mode: u32, backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 12u: { return set_lum(set_sat(source, sat(backdrop)), lum(backdrop)); }
        case 13u: { return set_lum(set_sat(backdrop, sat(source)), lum(backdrop)); }
        case 14u: { return set_lum(source, lum(backdrop)); }
        case 15u: { return set_lum(backdrop, lum(source)); }
        default: {
            return vec3<f32>(
                separable(mode, backdrop.x, source.x),
                separable(mode, backdrop.y, source.y),
                separable(mode, backdrop.z, source.z),
            );
        }
    }
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    let above = src[index];
    let mode = int_at(2u);
    let light = (int_at(3u) & 1u) != 0u;
    var weight = real_at(0u);
    if (int_at(3u) & 2u) != 0u {
        weight = weight * weights[index];
    }
    let below = dest[index];
    if mode == 0u && !light {
        let alpha = above.w * weight;
        dest[index] = vec4<f32>(above.xyz * alpha + below.xyz * (1.0 - alpha), alpha + below.w * (1.0 - alpha));
        return;
    }
    var source = above.xyz;
    var source_alpha = above.w;
    if light {
        let over_black = above.xyz * above.w;
        let brightest = max(over_black.x, max(over_black.y, over_black.z));
        if brightest > 0.0 {
            source = over_black / brightest;
            source_alpha = brightest;
        } else {
            source = vec3<f32>(0.0);
            source_alpha = 0.0;
        }
    }
    let alpha = source_alpha * weight;
    var backdrop = vec3<f32>(0.0);
    if below.w > 0.0 {
        backdrop = below.xyz / below.w;
    }
    let mixed = mix_channels(mode, backdrop, source);
    let alone = (1.0 - below.w) * alpha;
    let together = alpha * below.w;
    let under = 1.0 - alpha;
    let result_alpha = alpha + below.w * under;
    let colour = alone * source + together * mixed + under * below.xyz;
    dest[index] = vec4<f32>(clamp(colour, vec3<f32>(0.0), vec3<f32>(result_alpha)), result_alpha);
}
