// The warp: the CPU's backward mapping (`warp.rs`, `warp/footprint.rs`), pixel by pixel, never through hardware filtering.
// `src` is a straight RGBA raster; `dst` is the straight RGBA the layer lands on the output (zero where it does not reach).
//   ints:  0 out columns, 1 out rows, 2 source columns, 3 source rows, 4 mode (0 exact class, 1 affine footprint, 2 projective),
//          5 first output row, 6 footprint fixed (1) or per pixel (0), 7 plain (fixed footprint no larger than a pixel),
//          8 taps along the major axis, 9 taps along the minor one (fixed footprints)
//   reals: 0..5 the inverse map (a b c d e f), 6 and 7 the extent of a pixel across an edge (x, y) for affine maps,
//          8..11 the clip (low x, high x, low y, high y), 12 and 13 the lengths of the fixed footprint, 14 and 15 its direction,
//          16..18, 20..22, 24..26 the rows of the inverse homography, 28 the diagonal of the source
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;

const ROUNDING: f32 = 1e-6;

fn edge(inside: f32) -> f32 {
    return clamp(inside + 0.5, 0.0, 1.0);
}

fn share(low: f32, high: f32) -> f32 {
    return max(edge(low) + edge(high) - 1.0, 0.0);
}

struct Taps {
    first: u32,
    second: u32,
    toward: f32,
}

fn taps(at: f32, count: u32) -> Taps {
    let first = floor(at);
    let last = f32(count - 1u);
    return Taps(u32(clamp(first, 0.0, last)), u32(clamp(first + 1.0, 0.0, last)), at - first);
}

fn is_single(t: Taps) -> bool {
    return t.first == t.second || t.toward == 0.0;
}

fn pixel_at(row: u32, column: u32) -> vec4<f32> {
    return src[row * int_at(2u) + column];
}

fn accumulate(tx: Taps, ty: Taps, tap_weight: f32, mixed: ptr<function, vec4<f32>>, total: ptr<function, f32>) {
    for (var r = 0u; r < 2u; r++) {
        let row = select(ty.second, ty.first, r == 0u);
        let weight_row = select(ty.toward, 1.0 - ty.toward, r == 0u);
        for (var c = 0u; c < 2u; c++) {
            let column = select(tx.second, tx.first, c == 0u);
            let weight_column = select(tx.toward, 1.0 - tx.toward, c == 0u);
            let weight = tap_weight * (weight_row * weight_column);
            let q = pixel_at(row, column);
            *mixed += weight * vec4<f32>(q.rgb * q.a, q.a);
            *total += weight;
        }
    }
}

fn finish(mixed: vec4<f32>, total: f32, cover: f32) -> vec4<f32> {
    let alpha = mixed.w;
    var colour = vec3<f32>(0.0);
    if alpha > 0.0 {
        colour = mixed.xyz / alpha;
    }
    return vec4<f32>(colour, min(alpha / total, 1.0) * cover);
}

fn bilinear(x: f32, y: f32, cover: f32) -> vec4<f32> {
    let tx = taps(x - 0.5, int_at(2u));
    let ty = taps(y - 0.5, int_at(3u));
    if is_single(tx) && is_single(ty) {
        let q = pixel_at(ty.first, tx.first);
        return vec4<f32>(q.rgb, q.a * cover);
    }
    var mixed = vec4<f32>(0.0);
    var total = 0.0;
    accumulate(tx, ty, 1.0, &mixed, &total);
    return finish(mixed, total, cover);
}

// The footprint of a map whose inverse has the Jacobian [[a, b], [c, d]]: (plain, taps major, taps minor, lengths, direction).
struct Footprint {
    plain: bool,
    taps_major: u32,
    taps_minor: u32,
    lengths: vec2<f32>,
    along: vec2<f32>,
}

fn whole_up(length: f32) -> u32 {
    return u32(max(ceil(length * (1.0 - ROUNDING)), 0.0));
}

fn footprint_of(a: f32, b: f32, c: f32, d: f32, reach: f32) -> Footprint {
    let determinant = abs(a * d - b * c);
    let squares = a * a + b * b + c * c + d * d;
    let sum = sqrt(squares + 2.0 * determinant);
    let difference = sqrt(max(squares - 2.0 * determinant, 0.0));
    let major = (sum + difference) * 0.5;
    var minor = 0.0;
    if major > 0.0 {
        minor = determinant / major;
    }
    if major <= 1.0 + ROUNDING {
        return Footprint(true, 1u, 1u, vec2<f32>(major, minor), vec2<f32>(1.0, 0.0));
    }
    let first = a * a + b * b;
    let mixed = a * c + b * d;
    let second = c * c + d * d;
    let angle = 0.5 * atan2(2.0 * mixed, first - second);
    let lengths = vec2<f32>(min(major, reach), min(minor, reach));
    return Footprint(
        false,
        max(whole_up(lengths.x), 1u),
        max(whole_up(lengths.y), 1u),
        lengths,
        vec2<f32>(cos(angle), sin(angle)),
    );
}

fn footprint_sample(f: Footprint, x: f32, y: f32, cover: f32) -> vec4<f32> {
    var mixed = vec4<f32>(0.0);
    var total = 0.0;
    let across = vec2<f32>(-f.along.y, f.along.x);
    let share_of = 1.0 / (f32(f.taps_major) * f32(f.taps_minor));
    for (var major = 0u; major < f.taps_major; major++) {
        let along = ((f32(major) + 0.5) / f32(f.taps_major) - 0.5) * f.lengths.x;
        for (var minor = 0u; minor < f.taps_minor; minor++) {
            let aside = ((f32(minor) + 0.5) / f32(f.taps_minor) - 0.5) * f.lengths.y;
            let at_x = x + along * f.along.x + aside * across.x;
            let at_y = y + along * f.along.y + aside * across.y;
            accumulate(taps(at_x - 0.5, int_at(2u)), taps(at_y - 0.5, int_at(3u)), share_of, &mixed, &total);
        }
    }
    return finish(mixed, total, cover);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= out_columns() || id.y >= out_rows() {
        return;
    }
    let index = id.y * out_columns() + id.x;
    let centre_x = f32(id.x) + 0.5;
    let centre_y = f32(int_at(5u) + id.y) + 0.5;
    let mode = int_at(4u);
    var x = 0.0;
    var y = 0.0;
    var across_x = real_at(6u);
    var across_y = real_at(7u);
    var footprint = Footprint(
        int_at(7u) == 1u,
        int_at(8u),
        int_at(9u),
        vec2<f32>(real_at(12u), real_at(13u)),
        vec2<f32>(real_at(14u), real_at(15u)),
    );
    if mode == 2u {
        let first = vec3<f32>(real_at(16u), real_at(17u), real_at(18u));
        let second = vec3<f32>(real_at(20u), real_at(21u), real_at(22u));
        let last = vec3<f32>(real_at(24u), real_at(25u), real_at(26u));
        let point = vec3<f32>(centre_x, centre_y, 1.0);
        let depth = dot(last, point);
        if !(depth > 0.0) {
            dest[index] = vec4<f32>(0.0);
            return;
        }
        x = dot(first, point) / depth;
        y = dot(second, point) / depth;
        let ja = (first.x - x * last.x) / depth;
        let jb = (first.y - x * last.y) / depth;
        let jc = (second.x - y * last.x) / depth;
        let jd = (second.y - y * last.y) / depth;
        across_x = length(vec2<f32>(ja, jb));
        across_y = length(vec2<f32>(jc, jd));
        footprint = footprint_of(ja, jb, jc, jd, real_at(28u));
    } else {
        x = real_at(0u) * centre_x + real_at(1u) * centre_y + real_at(2u);
        y = real_at(3u) * centre_x + real_at(4u) * centre_y + real_at(5u);
    }
    let cover = share((x - real_at(8u)) / across_x, (real_at(9u) - x) / across_x)
        * share((y - real_at(10u)) / across_y, (real_at(11u) - y) / across_y);
    if !(cover > 0.0) {
        dest[index] = vec4<f32>(0.0);
        return;
    }
    if mode == 0u || footprint.plain {
        dest[index] = bilinear(x, y, cover);
    } else {
        dest[index] = footprint_sample(footprint, x, y, cover);
    }
}
