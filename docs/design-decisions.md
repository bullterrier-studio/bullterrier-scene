# Design decisions

This project's own decisions and their reasons, one section each: what was chosen, why, and what it costs. The
template's are in [template-decisions.md](template-decisions.md). This file is the project's: an update never changes it.

## Scene is a package of its own (2026-11-08)

Compositing is neither image's (one picture in, one out; image may not depend on vector) nor vector's (geometry, no raster
library) nor video's (decoding, encoding, sound and a timeline a still does not need). *Reason:* posters need layers across
image and vector, and video is unstarted, so splitting compositing out now is the cheapest moment. The options weighed are in
[design.md](design.md), "Why a package of its own".

## Scene knows image and vector (2026-11-08)

Unlike its siblings, scene depends on two of them, each in one adapter file (`source/picture.rs`, `source/drawing.rs`). It is
an integration package, as video will be. *Reason:* a layer must be drawn by the package that owns it, at the final size, so a
draft equals the print. *Consequence:* the repository is private until image and vector are released.

## Image and vector come in as git dependencies pinned to a revision (2026-11-08)

Neither crate is published (`publish = false`). A path dependency would tie scene's CI to a local checkout in the middle of a
branch. *Reason:* a pinned revision builds the same everywhere and a bump is a reviewed commit. *Consequence:* `deny.toml`
allows exactly those two git sources; a dependency arrives with its first user (C1), since an unused crate fails the gate.

## The pins: image `f319f13`, vector `5442066` (2026-11-13)

`Cargo.toml` pins `bullterrier-image` (`f319f138ffe11c7764f857f09c5610c367deb4bc`) and `bullterrier-vector`
(`5442066f9fa5e280ffab6f3a9f85456578900c7f`), the heads of their `main` when C1 began (vector's was `05debff`; C3 bumped it
for text), with no feature turned on (image's `onnx` stays off). *Vector's bump brought* V10e (`Window`: a rectangle of the
output drawn alone, the window drawing B1 and the C2 zoom limit wait for) and V10f (text runs: a text of several fonts and
sizes); the C3 text layer uses neither (one font, whole frame), and the `data_boundary` tests passed unchanged. Image's stays:
its `format` and region-map work is not pushed. *To bump:* change `rev` in a commit of its own, run `cargo test -p bullterrier-scene --test it
data_boundary` first (it reads the forms the siblings write with the pinned readers and fails naming what changed), then the
gate. `deny.toml` allows the two git URLs, `OFL-1.1` for `bullterrier-vector` alone (its bundled fonts), and the licences
their dependencies bring. *Cost:* a cold build of the two crates is about 35 s (dev) or 95 s (release wheel, 5.2 MB), and the
gate proofs peak at 5.1 GB. The repositories are private: a fetch needs git credentials, and CI needs a token that reads both (the workflows have none yet).
*For R1:* the wheel embeds vector's bundled fonts (OFL-1.1) while the package metadata says MIT.

## Image's range is nominal, scene's is exact (2026-11-13)

Image promises its pictures "nominally 0..1": a scanlines look on a checker gives -3e-14. The picture adapter clamps what image
gives into 0 to 1 where it becomes a `Raster`, and refuses a value that is not a number (`InvalidError` naming the look; for a
file, `ReadError`). *Reason:* scene's pictures are exactly 0 to 1 (a `Raster` refuses anything else, and the render's straight
alpha divides by an alpha that colours never pass), and this is the one place an outside contract with a tolerance enters.
*Consequence:* the render holds no clamp of its own.

## No WebAssembly build yet (2026-11-08)

Generated with `wasm = false`. *Reason:* nobody has shown that image's crate (rayon, rustfft, image decoders) builds for
`wasm32-unknown-unknown`, and scene's browser story is open. *Consequence:* `copier update` with `wasm = true` adds the gate
when a page needs it.

## Blending in sRGB values, straight alpha at the boundary (2026-11-08)

Vector blends sRGB values and returns straight alpha; the W3C spec blends the same way. *Reason:* a drawing must look the same
alone and in a scene. *Consequence:* linear-light blending, if wanted, is an explicit option; the scene premultiplies only
inside.

## Scene does not depend on control (2026-11-13)

Scene depends on image and vector only. Values reach it as plain numbers, text and pictures through `with_values`; a caller
that holds a control signal passes `signal.at(t)`. *Reason:* control answers "what is this value at time t?", and scene has no time
(a parameter is a state), so no scene type could hold a signal; the parameter description format is image's, not control's; the
mapping from signals to parameters is video's. A signal-or-number parameter would give two ways to pass a value, put a clock into the
cache key of C7, and make the render depend on a package nothing in it uses. *Consequence:* the design README and the ecosystem pages
say "scene knows image and vector"; no batch reads a control type. Adding the dependency later is additive and non-breaking; removing
it after a release is breaking. *Reopen when* something other than a caller must sample a scene parameter over time, and say then
which package owns it.

## Looks and drawings enter as data (2026-11-13)

A look is taken by its `to_dict()`, a drawing by its `to_json()`, in code and in JSON; a saved scene embeds both. The Rust
public API takes serialised data, not `bullterrier_image::Recipe` or `bullterrier_vector::Drawing`. *Reason:* scene's extension
links its own pinned copy of both crates and cannot take another extension's Python objects; keeping the crates' types inside
`source/picture.rs` and `source/drawing.rs` keeps them out of scene's semver; a scene file is self-contained. *Consequence:* a
look that holds a `Picture` or `Lut` object cannot be saved (a path form can); a version the linked crate does not read is a
`FormatError` naming both; a path to a recipe or drawing file is a read-only convenience, embedded when read.

## Scene and Layer are immutable values (2026-11-13)

Every call returns a new scene or layer, as vector's `Drawing` and image's `Recipe` do. *Reason:* a cache key per layer (C7) is
its value, a layer used in two places cannot alias, and a scene saves and renders the same wherever it is held. *Consequence:*
a layer drawn twice (a logo and its glow) is a reference (`Layer.of`), not a copy, so its parameters exist once.

## One way to set a value: `with_values` (2026-11-13)

A parameter is set by `scene.with_values({address: value})`, which returns a new scene; `render(size)` takes no values. Footage
frames, a template's photo and title are ordinary parameters of kind `picture` and `text`. *Reason:* two ways (`render(size,
values)` and `with_values`) would make the cache key and the saved form depend on which was used; image's `render` and
`with_values` are split the same way. *Consequence:* the per-frame loop is `scene.with_values({...}).render(1080)`.

## Where the entire work ends (2026-11-13)

A1, C1 to C7, M1, M2, M3, O1 (PNG only, through image's save: PDF and print colour wait until the owner wants a print), P1 (a Rust
command line crate that the Python console script runs, as image does) and the final poster. **B1 (bands) is deferred:** it needs
vector's V10e (not built) and image effects that take an origin and a canvas size, and the render interface does not change
when it arrives, so the API is final without it. The peak memory of a 15-layer A3 poster is still measured whole in C7 and
recorded. R1 (the release and the first push) is the owner's. Storage precision of finished layers is settled at B1.

## A layer drawn again is a reference, not a copy (2026-11-13)

`Layer.of("logo")` draws the named layer of the same scene again. It inherits that layer's source, `fit`, looks, own-map masks
and `place` (so those parameters stay at the named layer's address) and owns its layer masks, light, opacity and blend, with
looks, masks and a `place` of its own added after. *Reason:* a copy (`logo.look(bloom)`) has its
own addresses, so a caller sets every moving value twice and a forgotten one makes the glow drift from the logo; a reference
cannot. *Why masks are not inherited:* layer masks are scene composition, like opacity and blend; inheriting them made a cutout
of a clipped photo carry the clip, and let a mask lead back to the layer it masks. A shadow, a reflection, a glow and a mask by the subject are the same move. *Consequence:* references are resolved at
`add` (unknown names and cycles refused by an iterative walk, so no recursion depth is promised); a reference has no source and
no `fit`; things that must move together as one rig are an inner scene used with `Layer.scene`.

## Light is an explicit choice, not a rule on the blend name (2026-11-13)

A layer is light or not (`as_light`, the parameter `<layer>.light`): a light layer's alpha is the largest of R, G, B of its
look's colour over black, so a bloom shows past the shape. *Reason:* tying spill to `screen` made masks and the alpha of the
source disappear for one blend name and hid why a halo showed; a named choice is one parameter (`<layer>.light`) that a
switch can set. *Consequence:* dark light past a shape (a drop shadow) still needs image to carry alpha through effects; a
shadow is a fill masked by the shape.

## A mask is a layer or a named map of the layer's own picture (2026-11-13)

`mask(by)`: `by` is a `Layer` (canvas space, possibly `Layer.of`) or the name of one of the layer's own `maps` (picture space,
cut with the picture). A map is a parameter of kind `picture` (`<layer>.map.<name>`), so a footage frame with its matte, or a
template's cut-out, is driven by `with_values`. *Reason:* a raw array in a mask could belong to another layer's picture and
moves with the wrong layer; passing the matte once, with the picture, keeps it aligned, addressed and in the saved form. A mask that is a layer needs no new
kind: anything that can be drawn can mask. *Consequence:* a mask layer is owned by its layer and outside the scene's names.

## Placement is fixed fields with valid ranges (2026-11-13)

`place` sets `dx`, `dy`, `scale`, `aspect` (the height ratio), `rotate`, `skew`, `flip` and `anchor`; the order of application is
fixed, and the finished raster is put on the canvas by one warp ("Placement: a hand-written warp"). A layer is looked at its
fitted size, so a look's `px` is a canvas pixel as the layer is fitted and `place` scales a halo with its layer. *Reason:*
every placement number has an address (a chain of verbs cannot), and every range holds only valid values, so a slider built
from `parameters()` cannot ask for an error by itself (a zero or negative scale, a flip as a negative number; skew is applied
x then y, so its bound of 89 degrees is enough). A rule across two numbers of an embedded look is the sibling's. `fit`'s slack is `align`,
the pivot of `place` is `anchor`: one name per concept. `scale` is one number because it is what an animation drives.

## Placement: a hand-written warp, convex kernels, drawn at the placed density (2026-11-13)

**Crates surveyed** (crates.io and the fetched sources, 2026-11-13). Criteria: f32 channels with no 8-bit round trip,
transparent outside the source, a stated pixel-centre convention, sub-pixel accuracy, a cost that fits `deny.toml`.

| Crate | Finding | Verdict |
|---|---|---|
| `imageproc` 0.27.0 (MIT, 2026-06) | `warp` takes affine and projective maps over `f32` pixels, bilinear and bicubic, a constant border. But a pixel's value sits at its integer index (scene's centres are at +0.5), the map is evaluated in `f32` (a 90 degree turn is not an exact permutation), there is no prefilter (a reduction aliases), a border fades over one source pixel (a blur at `scale` 3, not coverage), and it brings 51 crates (nalgebra, rand, simba, wide) | not adopted, and not a dev-dependency either (51 crates in every gate-proof copy): the tests use hand-derived oracles (permutations, the rotation matrix, areas, area averages) |
| `fast_image_resize` 6.1.0 (MIT or Apache-2.0, 2026-07) | `f32` RGBA with alpha handling and fractional crops, SIMD; it scales and crops, with no rotation, skew or translation by a fraction | dropped: sources draw at the placed density, so scene never scales a finished picture up |
| `tiny-skia` 0.12.0 (BSD-3-Clause, 2026-02) | affine and bilinear draws, but a `Pixmap` is premultiplied 8-bit | fails the first criterion |

**Chosen:** one kernel in `warp.rs` (about 200 lines of code): backward mapping in `f64`, premultiplied `f32`, bilinear where a source
pixel is larger than an output pixel and the average of the output pixel's footprint where the map shrinks ("One footprint rule"), colour from the clamped edge and an alpha coverage by the
distance of the pixel centre to the box edge, transparent outside. *Reason:* the properties placement is tested on (a
whole-pixel move, a 90 degree turn and a flip are exact permutations; a half pixel is the average of two neighbours; the
area of a layer scales with the square of `scale`) hold by construction only when the arithmetic is ours. *Cost:* a kernel to
keep, no SIMD and one thread: measured in a release build on an arm64 Mac, a 12 megapixel picture on a 12 megapixel canvas
takes 0.05 s moved, 0.12 s scaled and 0.19 s turned by 17 degrees and scaled, at 530 MB peak, and
cutting it into 720 by 1280 takes 0.03 s (not promised, `api.md` "Limits, measured").

**Convex kernels only** (bilinear, area; no negative lobes). Scene's colours are exactly 0 to 1 with no clamp in the render
(see "Image's range is nominal"); that holds because every weight is at least 0 and the colour and the alpha are summed in the
same order, so rounding keeps the colour at most the alpha. A bicubic or Lanczos upgrade must restore a clamp, knowingly.

**Exact angles.** `rotate` wraps by `rem_euclid(360)` and 0, 90, 180 and 270 use exact cosine and sine; the matrix is built
from the factors and inverted by its own algebra, so a layer turned by a multiple of 90 degrees about a point on the pixel
grid is bit for bit a permutation. Other angles take `sin_cos` and are exact to rounding.

**Fractional boxes: one rule** (`geometry.rs`). The output picture covers the canvas exactly: `ceil(canvas x size / long
side)` pixels, stretched, so a design length is `columns / width` output pixels along x and `rows / height` along y (the
two differ by less than one pixel over the canvas, and are equal whenever the canvas is whole at that size). A rectangle of
design pixels drawn at a density is `ceil(width x density)` by `ceil(height x density)` pixels that divide it evenly, and the
warp puts that raster where its true rectangle falls, so a box between two output pixels is exact and the extra pixel is
never a stretch. A layer that covers the canvas covers it whole, at any size. On a thin canvas (100 by 3 at size 7 is 7 by 1 pixels) the two
scales differ by a large ratio and the design pixel is visibly not square: stated in `api.md` as a limit. *Rounding the short
side to the nearest pixel was weighed and not taken:* it would halve the error of an ordinary canvas (at most half a pixel
against one) and leave a thin one as it is (0.2 rounds to 0 and is raised to 1), but it changes a rule C1 published and
tests (a side of the output is the least whole number that holds its length) and every frame size a caller has pinned, for a
sub-pixel gain; it can be taken later, as a breaking change, on its own. A length that is whole in exact arithmetic but a few units in the
last place over it in a float (a fit factor times a zoom) is not rounded up a pixel (a relative error of 1e-12 is allowed:
`whole_up`). The binding axis of a `fit` ends at the box exactly, not at a product.

**Drawn at the placed density.** A source draws at `output scale x fit factor x place.scale`, so a vector headline at
`scale=3` is sharp (not an upsampled raster) and a look runs at the size the layer has, which is what makes a halo follow
`scale`. `aspect`, flip, skew, rotate and `dx`, `dy` are then one warp of the finished raster. For a picture with no look the
source hands over its own pixels and the warp is the only resample (source, fit and place fold into one). *Cost:* an animated
`scale` redraws the source and its look every frame, while `dx`, `dy`, `rotate`, `skew` and `flip` only re-warp a raster
that can be cached (C7 relies on it). A large `scale` on a big layer meets the 2^27 drawn-size limit even where little of it
shows: window drawing (B1) lifts it, and it is not worked around. After C5 gives looks design pixels, the scale handed to
image is `canvas scale x place.scale`, so a halo still follows `scale`.

## One footprint rule for a warp that is not an axis-aligned scale (2026-11-13)

C2a prefiltered by the length of each column of the map: right where the map keeps the raster's axes, wrong for a skew (a skew
of 80 degrees keeps both columns longer than 1 while the sheared stripes are far finer than a pixel, and they came out as a
beat). C2b replaces the rule for every map that is not of the **exact class**, and keeps C2a's for it.

- **Exact class:** an affine map each column of which has one entry that is not 0 (a move, a scale, `aspect`, a flip, a quarter
  turn, stretch, any product of them). The footprint of an output pixel is a rectangle along the raster's axes, and C2a's
  separable area average is it computed exactly; the tests that pin that (`a_reduction_is_the_average_of_the_pixels_it_covers`,
  `a_ramp_is_averaged_by_area_when_it_is_reduced`, the stripes of `aspect`, `stretch_fills_the_box_with_a_scale_for_each_axis`,
  the cover crops that reduce) hold to 1e-5 and 1e-6 against a pixel-by-pixel box average. *A pyramid for these would break
  them*: its levels are 2, 4, 8... pixels, and a reduction by 3 or 1.56 is a blend of two levels, not the box average.
- **Every other map** (a turn by another angle, a skew, a perspective) gets the footprint of the inverse Jacobian, per pixel
  for a perspective: singular values `s1 >= s2` (raster pixels per output pixel along the major and minor axes) and the major
  axis. `s1` at most 1 (up to `ROUNDING`, the one tolerance of the geometry: a 17 degree turn computes 1.0000000000000002):
  plain bilinear, so a turn at scale 1 is as sharp as in C2a. Otherwise the mean of a lattice of `ceil(s1)` by
  `ceil(max(s2, 1))` bilinear taps over the rectangle `s1` by `s2` along the footprint's axes, equal weights, so the filter
  stays convex. The taps are never more than a raster pixel apart, so the mean is the raster's average over the footprint
  whatever its direction.
- **No tap cap.** The only bound is the raster: a footprint longer than its diagonal reads the edge past it. Where both axes
  of a footprint are at least a pixel the footprints tile the raster, so the taps of a layer total about its pixels. Where the
  minor axis is under a pixel (a skew) each output pixel takes `ceil(s1)` taps and the total is about the output pixels
  times `ceil(s1)`: a skew of 89 degrees takes 57 taps a pixel (the sheared picture is mostly off the canvas, so the
  measured render is 0.22 s), and a degenerate quadrilateral takes at most the raster's diagonal each way for the few
  pixels it covers (12 megapixel degenerate quads were measured in review at 0.5 s).
- **Known limit: moire at mid-distance.** The lattice is a box average with bilinear taps, not an ideal low-pass: on the
  checker floor of `c2b_perspective.py` a faint set of arcs remains at mid-distance, where the checks are about two pixels. An
  isotropic or elliptical (EWA) filter with a smooth kernel is the later improvement; the kernel is not changed for it now.
- **Pyramid weighed and not taken.** The first build of C2b was a pyramid of 2 by 2 averages, trilinear at `log2(s2)`, with taps
  along the major axis only. Measured on the receding checker floor of `c2b_perspective.py` against a brute-force 8 by 8
  supersample of the continuous checker (mean absolute error per channel, the far rows near the horizon, then the middle): the
  pyramid 0.0297 and 0.0091; the pyramid one level coarser (`ceil`) 0.0411 and 0.0114; half a level coarser 0.0401 and 0.0109;
  the lattice 0.0271 and 0.0088. A coarser level blurs without removing the beat (the 99th percentile of the far error is 0.45
  for all four: the orange lines, one pixel thin, which no filter can both keep and average), and a pyramid's level is a
  box of 2, 4, 8 pixels that leaves part of a pattern whose period is not a power of two. The lattice needs no pyramid, so its
  peak memory is the picture's alone.
- **Isotropic level** (`log2(s1)`, no taps) was weighed on paper: it blurs a skewed or tilted picture by `s1 / s2` along its
  minor axis, which is the far side of every perspective floor.
- **Measured** (release, arm64 Mac, 12 megapixel picture on a 12 megapixel canvas; C2a, then C2b): turned 17 degrees and
  scaled 0.9, 0.19 s and 530 MB then 0.46 s and 386 MB (every pixel is four taps where C2a averaged the raster once first);
  turned 17 and scaled 0.5, 0.08 s then 0.16 s; skewed 80 degrees, 0.07 s then 0.23 s; skewed 89, 0.05 s then 0.22 s (57 taps a
  pixel; the sheared picture covers little of the canvas); under a perspective a tenth as wide at the far side, 0.50 s and
  386 MB.

## The feather is three box passes, not an exact Gaussian (2026-11-13)

A mask's `feather` is the standard deviation of a Gaussian blur, in design pixels, and a poster asks for a large one (300 design
pixels on a 2,400 pixel poster). **Measured** before choosing (release build, arm64 Mac, one thread; an exact separable Gaussian
cut at 4 sigma, as image's effects cut theirs, replicated edges, one `f32` plane of 2,400 by 3,200, in a scratch crate), and
then the kernel that ships, as a whole render of a 2,400 by 3,200 poster (its mask drawn on a plane with the margin the blur
needs, so a plane of 2,400 + 2 x 3 sigma by 3,200 + 2 x 3 sigma):

| sigma | exact separable, one plane | the shipped kernel, the whole render |
|---|---|---|
| 3 px | 0.08 s | 0.16 s |
| 30 px | 1.1 s | 0.22 s |
| 300 px | 13.1 s (with the margin, a plane 2.7 times larger: about 35 s, extrapolated) | 0.80 s (190 to 240 MB) |

The exact cost grows with sigma, the boxes' does not. **Chosen:** the plane is blurred by three passes of one box, along x and
then along y. The box has real half-width `h = n + f` (`n` whole, `f` from 0 to 1): pixels within `n` weigh 1 and the two
beyond weigh `f`, all over `2h + 1`. `h` has a closed form: with `t = sigma^2 / 3` (one pass's variance), `n =
floor((sqrt(1 + 12 t) - 1) / 2)` and `f = (t (2n + 1) - n (n + 1) (2n + 1) / 3) / (2 (n + 1)^2 - 2 t)`, so the three passes'
variance is exactly sigma squared (whole widths would put a sigma of 2 out by 9%) and there is no solver tolerance. Every
weight is at least 0 and they sum to 1, so the kernel is **convex** and a mask stays within 0 to 1 without a clamp. Against the
Gaussian the kernel's distribution function is off by at most 0.0099 (sigma 5 to 100: 0.0091 to 0.0099; sigma 2: 0.0052), which
is the tolerance of the tests that compare a blurred edge with the Gaussian's. Four passes gave 0.0074 and cost a third more:
not taken.

**Deviation from the plan's cut at 4 sigma.** The plan cut the Gaussian at 4 sigma as image's effects do (`truncate` 4.0). There
is no Gaussian here to cut: the three passes reach `3 ceil(h)` pixels, about 3 sigma, as a support that is exact (nothing
outside it, no renormalisation). The kernel is image's in variance and not in its tail: image's mass beyond 3 sigma is 0.27% of
a full Gaussian and ours is 0.

**Why a sum without subtraction.** A running sum (add the pixel entering, subtract the one leaving) leaves a residue of 1e-16
where a window of zeros follows a window of ones, and a negative alpha is not a picture. Each pass takes the window sum from a
block prefix and a block suffix of the line (both sums of non-negative numbers, accumulated in `f64`, van Herk and Gil-Werman),
so a value of a mask never falls below 0 and a window of ones sums to exactly its length (a mask by white stays 1 to the bit).

**Edge.** A line is extended by its end value (replicated) beyond its raster. A mask drawn on the canvas is drawn on a plane
larger than the canvas by the kernel's reach on each side (the margin), so the replicated edge is never read inside the canvas
and a shape that crosses the canvas edge feathers as it would on a larger canvas; a layer that is a fill covers the margin
too, so a fill stays 1 to the edge, and a shape that ends exactly at the canvas edge is 0.5 there. A map of a picture is
blurred on its own raster, where replication is the rule ("What is built", C3).

**Crates surveyed** (crates.io, 2026-11-13). Criteria as for the warp: `f32` channels, convex, a stated edge, a licence that
passes `deny.toml`, few dependencies.

| Crate | Finding | Verdict |
|---|---|---|
| `libblur` 0.24.0 (Apache-2.0 or BSD-3-Clause) | `f32` Gaussian and fast-Gaussian blurs with edge modes; an integer radius (the variance of the fast kernel is not sigma squared), rayon, `novtb`, `num-complex`, `zaft`, `unsafe` SIMD | not adopted: the kernel must have a stated variance, and four more crates and a thread pool for 150 lines |
| `fastblur` 0.1.1 (MIT) | three box passes, `u8` RGB only, 2019 | fails the first criterion |
| `blurslice` 0.1.0 (MIT) | the same method over `u8` slices | fails the first criterion |
| `imageproc` 0.27.0 (MIT) | exact `gaussian_blur_f32`, so the cost above; 51 crates ("Placement") | not adopted |

*Cost:* an approximation of the Gaussian within 0.0099, stated in `api.md`, and a kernel of ours to keep. A 16-deep nest of
mask layers, each feathered by 30 design pixels, draws a poster in 8.6 s and 630 MB (each level's plane is larger than the one
it serves by the reach of its blur).

## Text options are vector's, passed as data (2026-11-13)

`Layer.text(content, *, font, color, **options)`: scene owns the content (a text parameter), the font and the colour; the rest
is `btvec.text`'s, validated by the linked vector crate. *Reason:* restating vector's arguments and defaults in scene would
copy them and let them drift; the layer exists so a title can be a parameter. *Consequence:* only plain data crosses (no
expressions, no `anchor`, no `inside`); anything richer is built with vector and used as a `Layer.drawing`.

## One in-process cache, shared by derived scenes (2026-11-13)

Scenes derived by `add` and `with_values` share a bounded cache of what each layer drew; the result depends only on the scene,
its values, the size and the content of the files. *Reason:* a frame loop changes one value and must redo only what depends
on it (C7); the cache cannot belong to a scene value, which is immutable, nor to the caller. *Consequence:* a file is keyed by
path, size and modification time and its digest is computed at `to_json`; the cache is tested through a plugin layer that
counts its `draw` calls (`Layer.make`, arrives with C7).

## Smaller choices of the API (2026-11-13)

- **A group is a scene, a mask is a layer.** No group type and no mask type; one `Layer` does both.
- **One `Layer.picture`.** A picture of `None` is a hole the caller fills, so footage needs no second kind of layer.
- **Gradients are vector's.** A rectangle filled with `btvec.linear_gradient`; scene has no second gradient.
- **`Path` is a file, `str` is text or a name.** A mistaken `str` is a `TypeError` naming the fix, not a guess.
- **A look that sets `size` or `frame` is refused.** Scene owns a layer's size and fit; regions are fine.
- **Equality is structural.** Two scenes are equal when they hold the same data; equal scenes with equal values render equal
  pictures.

## Needs from the template: two template-owned edits (2026-11-13)

C1 edited two things the template owns, so `copier update` will conflict there until the template takes them. (1) `[tool.run-capped]
cap-mb` went from 4000 to 8000: the cold gate proofs peak at 5.1 GB of summed RSS (261 tests, a cold build of image and vector in
each copy; the Rust tests alone 2.7 GB), and the cap is that plus half again. A per-project value in a project-owned table would
need no conflict. (2) The stub proof rows in `tests/tools/test_rust_gates.py` named the removed `scale`; they now edit `MAX_LAYERS: int`
(a module-level ghost function) and `Layer.fill` (an extra parameter with a default, so that mypy passes and stubtest fails). The
template could name a seed line the project owns, as for `check_installed.py`.

Not done, for the same reason: the two tests that run `Layer.look` and `Layer.drawing` with the real image `Recipe` and vector `Drawing`
skip (`importorskip`) in scene's environment. Making them run means the two packages as dev-group git dependencies (`[dependency-groups]`
and `[tool.uv.sources]`, pinned to the crates' revisions). That works (cold `uv sync` 91 s, 4.5 GB peak summed RSS) but breaks the
template's `poe audit`: `uv export` writes the git requirements without hashes and `pip-audit --require-hashes` refuses them, so the
task needs `--no-emit-package` for both, an edit to a template-owned task. CI also has no token for the private repositories. Both
tests passed in a scratch environment with the two packages installed at the pinned revisions.
