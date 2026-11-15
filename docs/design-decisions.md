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

## The pins: image `7f91eef`, vector `5442066` (2026-11-13; image bumped 2026-11-14 twice)

`Cargo.toml` pins `bullterrier-image` (`7f91eefbc4695ae20d01dc2da3de1eaadc13a274`, which has `Recipe::spread` for C5c; before it `c6b58ac`; C1 began at `f319f13`, C5b-lite moved it to image's pushed `main`, with a canvas smaller than the picture allowed (a layer larger than the canvas): `Recipe.to_dict()` writes `format`, `RECIPE_FORMATS` is public, `Options::with_canvas`; the private copy of image's format list is gone) and `bullterrier-vector`
(`5442066f9fa5e280ffab6f3a9f85456578900c7f`), the heads of their `main` when C1 began (vector's was `05debff`; C3 bumped it
for text), with no feature turned on (image's `onnx` stays off). *Vector's bump brought* V10e (`Window`: a rectangle of the
output drawn alone, the window drawing B1 and the C2 zoom limit wait for) and V10f (text runs: a text of several fonts and
sizes); the C3 text layer uses neither (one font, whole frame), and the `data_boundary` tests passed unchanged. *To bump:* change `rev` in a commit of its own, run `cargo test -p bullterrier-scene --test it
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

A1, C1 to C7, M1, M2, M3, O1 (PNG through image's writer; a raster PDF), P1 (a Rust
command line crate that the Python console script runs, as image does) and the final poster. **B1 (bands) was deferred here** (it needed
vector's V10e and image effects that take an origin and a canvas size) **and is built since**, within what the two give: "Banded rendering (B1)" below.
R1 (the release and the first push) is the owner's. Storage precision of finished layers was settled at B1: float32.

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
fitted size, so a look's `px` is image's design pixel (1/480 of the canvas's long side) as the layer is fitted and `place` scales a halo with its layer. *Reason:*
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
shows: B1 lifts it for a layer that draws only its own pixels, and a group drawn by band ("Banded rendering (B1)"). After C5 gives looks design pixels, the scale handed to
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

## Masks, maps and type: what C3 settled (2026-11-13)

- **A map is the picture's, and a look is handed it.** A picture layer holds its maps (held, a file, or a hole) and checks them
  against the picture when the scene renders (size, hole). A look gets the maps its recipe can be given (`Recipe::known_maps`:
  image refuses a photo handed a map that is no map of the recipe), resampled with the picture's window and kernel through
  image's `Photo::with_map` (the plan said `Options::with_map`; at the pin that is a method of the photo). A scene never runs
  a finder, so a look that needs a map the layer lacks fails in image's words, naming the layer, the step and the map.
- **Stage 4 acts where the raster is.** With no look the map is multiplied into the picture's own pixels (one copy of the
  picture), then the one warp cuts, fits and places both; with a look it is resampled to the look's size and multiplied after
  the look. *Reason:* the picture and its map move together by the same warp, so C2's "source, fit and place are one resample"
  holds, and a no-op look and no look agree (to the bit at one pixel to the design pixel; "What is built"). *Cost:* one copy of
  the picture where a mask by a map is used, and a feather blurred at the picture's resolution (cheap: its cost does not grow
  with sigma).
- **Stage 6 is a factor at the landing pixel.** The product of a layer's masks is one `f32` plane of the canvas plus margin;
  the warp multiplies each pixel it lands by the plane's value there, as it does opacity. No canvas-sized raster of the layer is
  made, and a layer with no masks runs the code it ran before (measured: 1,000 fills 0.67 s before and after, "What is built").
- **A mask layer is drawn on a target with a margin.** The plane a mask is drawn on is the canvas plus the reach of its blur on
  each side, so a shape that crosses the canvas edge feathers right. *Alternative weighed and not taken:* replicate the
  mask's edge (cheap, and wrong for a shape that crosses the edge). The margin costs a plane up to `(1 + 6 sigma / canvas)^2`
  times the canvas, `(1 + 6 sigma / width) (1 + 6 sigma / height)` (a poster with sigma 300: 2.7 times), and is bounded by 2^27
  pixels. Two things make it work: the warp's
  output may start before the canvas (`Map::shifted`, additive), and a source with no box of its own (a fill) with no `fit` and no `place` is asked for the canvas
  grown by the margin, so it covers it (`mask(fill)` stays 1 to the edge, and a shape that ends at the canvas edge is 0.5
  there); a fitted or placed fill is asked for the canvas, because `fit` and `place` act on the box and a layer must be the same
  shape whatever its blur adds. A map has no margin: it is blurred on its own raster with the edge replicated, because a matte that touches the
  picture's edge goes on past it.
- **A source's box may start away from the origin.** A text's box is its glyphs' bounds, which start left of and above the
  anchor, and a fill's box is the margin canvas. `fitted`'s `cover` window had assumed an origin of 0 (`window.x = (width -
  w) x align`); it adds the box's `x` and `y`. The rest of `Layout` already read the box's own rectangle.
- **Mask layer rules are refusals.** A name, and when they exist a blend and `as_light`, are `InvalidError` at the `mask` call;
  the opacity multiplies. *Reason:* a name would collide with C4's lookup of a reference, and a silent ignore of a blend or a
  light would hide that a mask has none.
- **Nesting is counted at construction.** `MAX_DEPTH` (16) and a count of mask layers (at most `MAX_LAYERS`, a layer used twice
  counted twice, since a layer that masks by two copies of the one below it would draw it 2^16 times) are checked at the `mask`
  call, so a structure that is too deep or too wide cannot be built. The walks over a nest recurse to its depth and no
  further (L2 of the shared lessons): measured, a nest of 16 is drawn, printed, cloned and dropped in 64 KB of stack unoptimised
  (32 KB overflows) and the test runs in 256 KB. A mask's `Debug` prints its depth, not the layers it nests (a layer shared
  between masks would print once per path).
- **Type is vector's text, as vector writes it.** The text layer asks vector for a text drawing with no options, takes the
  document vector writes for it, puts the options in as the plain data they are and has vector read it back: vector checks
  its own arguments in its own words, and scene restates none. *Why not vector's typed `Text` builder:* it would name each of
  vector's options in scene. *Why not a document written here:* a bundled font's bytes are not public, and its entry in a
  document is named by a digest scene cannot compute. The text is measured (its glyph bounds with the anchor at the origin),
  then set again with its anchor where the bounds start, in a frame of exactly the bounds, so that the layer's box is the
  bounds and its origin the anchor. A font file is read at the call, not at `render`: the box needs the font.
- **Codegen units.** The kernel of the warp is generic over its sink and is instantiated at its caller, but its small helpers
  (`share`, `taps`, `edge`) were plain functions: adding modules to the crate moved them into another codegen unit and every
  blended pixel paid a call (1,000 fills 0.67 s then 0.80 s, found by the regression measurement). They are `#[inline]` now.
- **A drawing is drawn to the pixel (review of C3).** Vector draws a frame at one scale (`min` of the two ratios), centred, in a
  raster whose short side is rounded up: a quarter to a half pixel of slack, so a full-frame drawing had a translucent border and
  `stretch` letterboxed. The adapter makes the raster the frame: the shapes are scaled by the density along each axis over the
  mean density (about 1.00x, or the two densities of `stretch`) with vector's own `scale` step, moved by `translate` by the
  fraction of a pixel the box starts at (`Request::phase`, worked out by `Layout` for a placement that only moves and zooms), and
  the frame is set to the raster over the mean density, so vector's one scale is the mean on both axes. The raster covers the box
  from that fraction before its top left, so the warp copies it onto whole pixels, and `ends_at_window` tells the warp not to cut
  the drawing's own antialiased edge again by coverage (a pixel at an edge of a text was counted twice: 0.81 for 0.9). A text's
  box is now exactly its bounds. *Cost:* a stroke, shadow offset or halftone pitch follows the mean density, not each axis, and a
  gradient, clip or mask of the drawing stays in its own frame (vector's own rule: "set the paint after the transforms").
- **A map's blur is bounded by its line, a canvas mask's by its plane (review of C3).** The plane check applied to a map, which makes
  no plane and blurs line by line, so a large picture with a hard mask or a feather that reached past 2^27 pixels in area was
  refused with a message about a feather. A mask with no feather is never refused for its size; a map's blur is refused only for
  a line (the picture's side and the reach on both sides) of more than 2^24 pixels, whose five `f64` buffers are 640 MB. A canvas
  mask keeps the plane check: it makes the plane.
- **Where a fill is given the margin is written once, in `landing.rs`** (`land`): a source with no box of its own, unfitted and
  unplaced, is asked for the canvas and its margin; anything else for the canvas. `landing.rs` is the module the renderer and
  the masks both use, so they do not import each other.
- **"Invert before feather" is not a mutant that can be killed:** the kernel is linear and its weights sum to 1, so blurring
  `1 - v` is `1 - blur(v)` to rounding, margin and replicated edge included.

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

**Edge.** A line is extended by its end value (replicated) beyond its raster, once, as far as the three passes reach, before the first pass (the review of C3 found it replicated again after each pass, which blended the end values and flattened a map that was narrower than its kernel: 0.52 for 0.64). A mask drawn on the canvas is drawn on a plane
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
counts its `draw` calls (`Layer::make`, built in C7: "The cache", below).

## The cache (C7, 2026-11-14)

**What is kept.** Four things, in one bounded LRU: a layer's raster before the warp (source drawn at the drawn density, looks and own-map masks run), a group's raster (it is a layer's raster whose source is a scene), the plane of a mask layer, and a picture or map read from a file. Not kept: the join (it is the cheap part: 0.025 s for a mill frame that hits everything) and the fused picture with no look (it already warps straight from its own pixels, bit for bit as before). *Reason:* "change one value, redo only what depends on it": a layer that only moves is warped again from its raster, which is the move-only fast path of `future.md`. The settled design had no third choice to make here.

**Where it lives.** In the `Scene` value as one `Arc<Cache>` made by `Scene::new`; `add` and `with_values` pass it on, so scenes made from one another share it and scenes made apart share nothing. No global state. `==`, `to_json` and `parameters()` do not see it (a test). A group draws its inner layers into the *outer* scene's cache (`Target::cache` is handed down), so there is one bound, and a layer inside a group is kept by what it is made of, whichever scene holds it. *Cost:* the inner scene's own cache is never used when it is drawn as a group.

**The key** is a SHA-256 of everything that reaches the value, written by whoever owns the input (`Source::keyed`, `Finish::keyed`, `Layer::keyed`, `Target::keyed`): the source's content, the looks' documents, `light`, and the request. A plain value (a placement, a request, a scale) is written by its `Debug` text, which writes every field, so a field added later is in the key without anyone remembering; a heavy one (a picture, a recipe, a drawing's document) is hashed once and kept with its owner (`OnceLock`). The one trait method `Source::keyed` has no default, so a new source cannot forget it. The completeness is tested, not argued: `tests/it/cache.rs` and `tests/test_cache.py` change every address of the example scenes on a scene whose cache has been used and compare with a scene made afresh, bit for bit, and the inputs that are no address (`fit`, a perspective, a mask's feather, inversion and reading, the order of looks) are added as layers. Eight hand mutants, each leaving one input out of a key (looks, source, phase, light, a mask plane's feather, a mask layer's opacity, a drawing's Param values, a map's digest), each fail the oracle and name the addresses.

**The phase stays in the key**, for a drawing, a text and a group (C3 draws them at the fraction of a pixel their box starts at, which follows `dx`): a layer that shakes by a fraction of a pixel draws its source again each frame, and runs its look again. *Measured:* in the mill, with every other value held, a logo moved by whole pixels takes 0.025 s a frame and one moved by a fraction of a pixel 0.029 s (its drawing and its glow's drawing and look redone); the wow loop's dog and keyline are pictures, which have no phase, so their sway costs nothing extra. So no render changes and no hash corpus is redone. *The alternative if a piece shakes a looked drawing that is large* (a bloom on a full-frame drawing): draw a looked layer at phase 0 and let the warp shift it, which softens it by one resample; that changes renders, so it is the owner's decision, with the piece that needs it. A picture and a plugin layer do not follow the phase (their key leaves it out), so a picture that moves is drawn once.

**The bound is 2 GiB** (`cache::MAX_BYTES`, bytes of pixels, planes and decoded files; the least recently used leaves first; an entry larger than the bound is not kept and nothing is refused). *Measured* on a 15-layer A3 poster at 300 dpi: the reusable layers are about 1.2 GiB; at 1.1 and 1.2 GiB the second render of the poster takes 5.0 and 5.2 s (a render takes the layers in turn, so least recently used always evicts the layer wanted next: no hit at all), at 1.25 GiB and above 0.39 s. 2 GiB is a third more than the poster needs, and the render's peak with it is 3.15 GB against 2.16 GB without (the cache keeps what the render would have freed). *Cost, and the limit:* a render that scans more than the bound keeps and reuses nothing and holds 2 GiB for it (12 looked fills at A3: 3.7 GB against 1.6 GB); a scan-resistant policy is the answer if a piece lives there, and no piece does yet. The bound is not a parameter: a hidden tunable has no place in a library whose result must not depend on it, and a caller who needs less renders a smaller size.

**Files.** A file is keyed by path, size and modification time; a hit skips the read, so it also skips the check of a saved digest. A change that keeps all three is not seen. A file a look's recipe names is keyed by its path in the recipe. *Reason:* hashing a 12 megapixel picture's file at each render costs what the render saves; the digest of a saved scene is checked at the read that follows a change.

**Hashing once.** A picture or map held in memory keeps the digest of its pixels (`Raster::digest`, a `OnceLock`), made on the first `==` or key, over pieces of 4 MiB on every core, so `==` of a 12 megapixel picture is no longer a fraction of a second at each call and a swapped picture costs its hash once. The first render of a layer with a look pays 22 ms for 3 megapixels. `Finish::run` takes `light` as an argument, so a draw no longer deep-clones every recipe of the layer.

**The plugin seam** is the smallest that the test needs: the public trait `LayerKind` (`name`, `options`, `draw(area, scale)`) and `Layer::make(Arc<dyn LayerKind>)`. **A kind declares its identity by its name and options**, which are also what a saved scene will write, so what the cache keys and what the file holds cannot differ; the promise (the same name, options, rectangle and scale give the same raster) is the trait's, and a kind with a hidden input must put it in its options. The scene asks for the part of the canvas that shows at the drawn scale, per axis, and refuses a raster of another size, naming both. *Not built then, built in C8 (below):* a registry, entry points, Python `register_layer` and `Layer.make`, and a JSON form; none changed the trait.

**A look chain is kept stage by stage (built).** A reference runs the named layer's looks and then its own, so changing the last look used to run the whole chain (a mill frame whose two swells change ran the night grade twice). `Finish::run` now keeps the raster after each look but the last, for a layer with two or more looks, and resumes from the deepest stage the cache holds. *The stage key* is the key rule of the finished raster cut at the stage: everything below the looks (the source with the digests of its picture and maps, `light`, the request with the phase and held pixels a source that draws on the grid has), the room the whole chain grows the raster by (a light layer's raster is grown by the room of all its looks before the first runs, so look `k + 1` is in the stage's key through the room), and the looks' documents up to and including the stage's. The masks and the maps' names are not in it (a mask runs after the last look; the maps' content is in the source). A reference's looks come after the named layer's, so the first stages' keys are the named layer's. *What a stage holds:* the raster, the covered part of the box, whether it ends at the window, the room shown, and the maps its later looks and the masks read, at the stage's size (grown with the raster). The maps are shared by the stages of one run; a stage is resumed from only if it holds every map the resuming chain reads (`uses_map` over the source's maps), else the source is drawn again and the stage is kept with the maps of that chain. Two chains that read maps neither of which holds the other's thrash one stage; nothing in the mill does. *Which stages:* none for one look (its stage is the finished raster); for `n >= 2`, `n - 1` stages. The finished raster is still kept by the layer's key, which also holds the masks. The named layer of a reference does not keep a stage (it keeps its finished raster): the first frame runs its look once more, when the reference's chain is first drawn. *Memory, and why stages are expendable:* a chain of `n` looks keeps up to `n - 1` more rasters of its layer's size. Measured on five 4,000 by 3,000 layers with three looks each (bound 2 GiB): stages and finished rasters together are 2.9 GB, more than the bound, and with stages as ordinary entries the render scans the layers, each insert evicts the least recently used entry, and the finished rasters left (a change of the last layer took 25.8 s against 4.5 s without stages). So a stage is expendable (`Cache::put`): it is kept only where it fits beside the other entries by making other stages leave, and any other entry makes stages leave first. A stage thus never costs a finished raster; a chain that does not fit keeps fewer stages and runs more looks, never more than before. The stage's maps are counted in each stage though shared, so the accounted bytes are at most those the cache holds. *Measured and unchanged:* every render of a corpus of 26 scenes (chains of one to three looks, references that add looks, light layers with room, a group and a mask layer with looks, masks by maps) is bit for bit what it was before; the key's completeness is tested as that of the finished raster is (`tests/it/cache.rs`, every address of a scene of chains, three-look chains, references adding looks, a light layer, another render size and back), and the looks that run are counted by an inline seam (`finish.rs`). Mutants of the key that the oracle kills: the key holding only the first look, the key without the room, a resume that ignores the maps a chain reads, a resume one stage too early, no resume.

## C8: layer kinds by name (2026-11-14)

Copied from the design of audio's measures by name (its A7), adapted to layer kinds; what differs is stated.

- **The registry is a value.** `LayerKinds` (`new`, `register`, `get`, `names`, `make`) is passed where it is needed; a `Scene` holds none,
  because a layer made through one remembers its kind and options. Python holds the one process-wide registry in the bindings,
  as audio does. Scene has no built-in kinds (its five sources are not plugins), so there is no `builtin()` and no built-in
  name to protect; what is protected is the keys that say what a layer draws.
- **A factory builds a kind from options.** `LayerKindFactory { name, identity, create(&options) }` is smaller than audio's
  `Measure` (no description, unit or scaling: a kind states none of them). `create` is what lets JSON recreate a layer; the
  `LayerKind` trait is unchanged. A factory that makes a kind of another name is refused at `make`.
- **Getting in is checked once**, in `register`: the name (`^[a-z0-9][a-z0-9_.-]*$`, at most 64 characters, as long as the
  names of a saved scene are kept short), not one of the source keys (`picture`, `drawing`, `text`, `fill`, `scene`, `of`,
  `make`: the same list the reader uses, so a flat spelling `{"<kind>": ...}` stays possible), and the identity rule: the
  same identity re-registered replaces itself (a module loaded again), another identity under a taken name is refused. A name
  in a message is cut at 40 characters, so a name of a megabyte is refused in a line.
- **The JSON form is `{"make": {"kind", "options"}}`**, not top-level `kind` and `options` keys: `options` already belongs to a
  text layer (`OWNED`), and one source key keeps "a layer draws one thing" true. The document was already what `==` compared.
  A layer made by `Layer::make(kind)` has no factory behind it and still refuses `to_json`, naming why. An unknown kind is a
  `FormatError` at `/layers/N/make/kind` (the registered names and the nearest, by the rule of `suggest.rs`); options the
  factory refuses are at `/layers/N/make/options`.
- **One Python callable, per-axis scales unpacked:** `draw(x, y, width, height, scale_x, scale_y, **options)`. The earlier
  draft of `api.md` had one `scale`; the core asks per axis (a fractional canvas makes the two differ by up to one part in the
  long side) and a size of `ceil(width * scale_x)` by `ceil(height * scale_y)` cannot be written from one number. Options
  travel as JSON text both ways (`json.dumps` in the facade, `json.loads` per call), so the callable sees plain data and
  cannot alter the layer's own.
- **A plugin's exception becomes an `InvalidError` naming the kind**, with its type and message, where audio re-raises the
  plugin's own exception unchanged. Audio needs the original because a measure runs on the caller's thread; a layer is drawn on
  render threads, where one exception among several has no caller to be raised unchanged to, and the error must carry the
  layer's address like every other refusal of `render`. A returned array is checked where it comes in: its shape before its
  pixels (more than 2^27 pixels is refused before a copy), then `Raster::rgba` (a NaN or a value outside 0 to 1, by row and
  column), then the scene's own size check.
- **Loading is lazy and safe**, as in audio: the group `bullterrier_scene.layers` is read by the first `layer_kinds`,
  `Layer.make` or `Scene.from_json`, never at import, sorted by name, under a lock; a module that fails leaves the registry as
  it was (snapshot and restore) and its half-imported modules forgotten, and the error names the entry point. The extension
  calls `plugin._load` at run time, so it still imports nothing of the package; `plugin` is a layer of the import contract
  above `_native`.
- **Threads.** Each call into Python takes the GIL (`Python::attach`) and the render runs without it (`detach`), so several
  renders in threads work; a registry is cloned for each query, so no lock is held while a kind draws.
- **The contract is a test over every registered kind**, found in the registry (`tests/test_layer_kind_contract.py`, and its
  Rust twin in `tests/it/layer_kinds.rs`): the size asked, values from 0 to 1, no NaN, deterministic, for rectangles and
  scales that include a fractional, a tiny and one beyond the canvas. A kind that breaks it is refused at render, with a
  known-bad kind of each sort in `tests/test_plugin.py`. Scene has no built-in kinds, so the tests register their own two.

## Blend modes and light: what C4a settled (2026-11-14)

- **A blend is applied at the landing pixel, by one function.** `blend::join` takes the pixel the warp lands (straight colour,
  alpha times masks and opacity) and the premultiplied backdrop and applies the equation of section 9 of the W3C Compositing and
  Blending Level 1: `Cs' = (1 - ab) Cs + ab B(Cb, Cs)`, `co = as Cs' + (1 - as) ab Cb`, `ao = as + ab (1 - as)`. *Reason:* the
  renderer hides blending, and a layer needs no canvas-sized raster of its own for it (C2a's rule). *Alternative weighed:* blend
  whole rasters after the warp (simpler to read, one canvas-sized copy a layer, 2^27 pixels at 2 GB each).
- **Normal keeps its code.** A layer that is normal and not light takes the three paths it took before C4a (masked, unmoved copy,
  warp); only the others go through `join`. *Reason:* the 1,000-fill benchmark is the measure of that path and L21 of the shared
  lessons says a new module can cost it. Measured: unchanged ("What is built", limits). *Cost:* a blended layer is three to six
  times a normal one, which copies its pixels (a 12 megapixel picture: 0.03 s normal, 0.09 to 0.18 s blended).
- **The result is held between 0 and its alpha, in `join`.** A property test found `luminosity` giving a channel of -7e-9: the
  clip of the non-separable modes lands on 0 or 1 exactly in real numbers and an `f32` misses by ulps. The straight backdrop of the
  next layer is that colour over its alpha, so the invariant "a premultiplied colour never passes its alpha" is made where the
  colour is made, once, not defended in the readers.
- **The brightness is valid by construction, not guarded where it is divided (review of C4a).** `color` and `luminosity` gave NaN for 14
  greys: `SetLum` moved a colour to a brightness, an ulp off for a grey in 32 bits, the colour fell below 0 by an ulp, and `ClipColor`,
  which measured the brightness again, found it equal to the least channel and divided 0 by 0. `ClipColor` takes the brightness `SetLum`
  aimed at, so `brightness - least` and `most - brightness` are positive whenever their branch runs (the aim is from 0 to 1: monotone
  rounding, white's `Lum` is exactly 1). *Alternatives weighed:* a guard at the division (the invalid colour stays and a branch copes);
  summing `Lum` in 64 bits (a first fix: it made a grey's brightness its level, which nothing needs once the clip is given its brightness; a
  mutant that took it back survived every test, so it was removed). The state space of greys and near-greys is finite, so it is swept whole.
- **Light is linear, so its order among the masks and the opacity is not observable.** `lit` turns the landed pixel into its colour
  over black, with alpha the largest of the three, and the masks and the opacity scale that alpha; a mask before or after the
  light gives the same pixel, and the code does it in the order of the stages. *Consequence:* a look that makes room past the box
  (C5) is what makes light spill; until then a light layer only counts the colour it has by its brightness.
- **A suggestion is made only for a word that is near a name.** One rule: the names the word begins (lower case, `_` read as `-`),
  else those fewest edits away if within a third of its letters (at least one), else none. It covers `"lumi"`, `"Multiply"`,
  `"soft_light"` and a typo, and gives `"bogus"` and `""` nothing (the nearest by distance alone had said `"hue"`). `Blend` prints its CSS name
  from the table `FromStr` reads, which C6 writes `<layer>.blend` with.
- **Vector is the differential oracle of the separable modes, and its tolerance is its own.** Vector holds a translucent source
  premultiplied in 8 bits, which `color-burn` and `color-dodge` turn into up to 2.1 levels at alpha 0.5 where the W3C formula
  and scene agree; the bounds are 2 levels for an opaque source and 2.5 for a translucent one (the largest seen are 1.86 and 2.16), too loose to see a boundary that
  moves by a level, so soft-light's boundaries, `ClipColor`'s thresholds and the translucent equation are also checked by hand-worked rows.
- **Mask-layer refusals are at the `mask` call**, as the name already was: a non-normal blend and `as_light(true)` are
  `InvalidError` naming `mask`; the calls that say nothing (`blend("normal")`, `as_light(False)`) are accepted.
- **What C4b meets** ("Needs from others" in `api.md`): `lay` draws into the one joined buffer and knows no layer by name; a group
  needs an isolated buffer; `Layer` does not separate the stages a reference inherits from those it owns; `Placement` cannot hold
  two placements composed.

## C4b: references (2026-11-14)

- **A layer is a body and a join, and a reference holds a name.** `Layer { name, base, join }`: the body (source with its looks and own-map masks,
  `fit`, placement, extra places) is stages 1 to 5, the join (layer masks, light, opacity, blend) 6 to 9. `Scene::add` resolves a
  reference into the body it draws from the *body* of the layer it names, so inheriting nothing of the join is a property of the types, not of a
  list of fields that a later field could be left off. *Alternative weighed:* a reference as a clone of the named layer with the join reset
  (it would inherit a field a new batch adds to `Layer` by default). The split went in first and its own commit proves no change: 45 scenes
  (pictures, drawings, text, fills, every placement and perspective, masks of every kind, sixteen blends, light) at two sizes hash the same.
- **References are resolved at `add`, into a second list.** The scene keeps the layers as added (`layers()` returns them: a reference
  stays one, and the same layer added to another scene is resolved there) and the layers as drawn (every reference replaced by a body,
  also in mask layers). `render` knows no reference exists and passes nothing by name. *Alternative weighed:* resolving at render through a
  name table passed down (no second list, but `lay`, `product`, `plane` and a repeated fold of the looks at every draw, which re-parses the
  recipe each time). *Cost:* every `add` clones both lists, a sum over at most 1,000 layers; it is what `add` already did once.
- **Extras are a list, composed at render.** The bounding rectangle a reference's `anchor` names is of the placed box, which depends on `fit` and
  on the source's real size, known only when the layer is drawn. So the body keeps the references' places as a list of `Placed` and `Layout`
  turns them into steps about the rectangle of what came before. An affine after an affine is an `Affine::then` in output pixels; after a
  perspective it is a 3 by 3 product with the quadrilateral's homography, whose last row survives exactly, so C2b's decision of what is affine
  stands and nothing is classified twice. The no-extras path is the code it was.
- **A cycle through a mask is refused although it would end.** A reference takes a body and no masks, so `a.mask(Layer.of("b"))` with
  `b.mask(Layer.of("a"))`, and a layer masked by a reference to itself, draw fine; the plan refuses them and so does `add`. *Reason:* a refusal
  can be relaxed later and a cycle accepted cannot be refused; looks on every layer (C5a) and groups (C4c) may make a mask part of what a
  reference draws. *Cost:* a layer cannot be cut by a copy of itself through `Layer.of`; use a second layer.
- **Depth is mask layers plus reference links, count is layers drawn.** One `Budget` (`budget.rs`: depth and layers, `beside`, `needing`,
  `through`, `checked`) stands for the rule, kept in the scene per add: C4c adds a group's inner budget to it (`Source::budget`). The count changed from C3: it
  counted the stack and one layer's mask layers apart (1,000 each), now a scene draws at most 1,000 in all, a mask layer used by 500 layers
  counted 500 times. The plan's example (600 layers and a mask layer used by 600 references) is refused by the stack alone and kills no
  mutant; the rows are 500 and 501 layers sharing one mask layer.
- **A reference redraws the named layer's body at the composite density.** A halo must follow an extra `scale`, so the source is asked at
  `output x fit x place.scale x extras' scales` (for a perspective, the largest scale of the composite homography). The cost is a second draw
  of the body, as a copy costs; only a reference that leaves the density alone can share a raster (C7).
- **No snap of near-zero entries.** The plan asked to classify a composite by the relative tolerance. For an affine after a perspective the
  rule is inherited (the last row). For an affine after an affine the entries are exact where the places are exact (quarter turns, flips,
  moves, powers of two); a turn of 30 degrees then 60 has entries a few ulps from a quarter turn's and would have to be snapped near 1 as
  well to be a copy, which `place(rotate=90)` is and the product of two turns is not: the general kernel gives the same pixels to 1e-4.
- **Shadow of the examples:** the shadow is a skewed, flattened reference under the figure, because the same flattened shape right under a
  figure is hidden behind it.

## C4c: scenes as layers (2026-11-14)

- **A group is a source, and one entry draws a stack.** `Layer::scene(inner)` is a layer whose `Source` holds the scene (`Arc`, so a use is a pointer);
  its `open` says the box is the inner canvas and its draw closure calls `render::draw(layers, target)`, the function `Scene::render` calls with
  the canvas as the target. One render path: nothing in the renderer knows a group exists, the layer that holds it is warped, masked and joined
  as any layer. *Alternative weighed:* a `Group` variant of `Base` that `lay` special-cases (a second path to keep equal to the first, and a place
  for B1's bands to miss). The cost is that the group's picture is made straight and premultiplied again (a group alone is its scene's render bit for bit
  where opaque, 2e-6 where translucent).
- **The target has an origin and a count.** `Target` gained `offset` (the fraction of a pixel the canvas starts at in the grid, which moves
  every inner layer's pixel phase and the shift of its map) and `held` (the pixels of the groups outside). Both are 0 for the whole canvas, and a
  mask plane inherits them through `grown`, so a layer mask inside a group is drawn on the group's grid for free. `Layout::phase` adds the
  offset to the position it takes the fraction of; `Request` carries `held` to the source. *Alternative weighed:* a group raster that
  starts on the box and is resampled by the outer warp (softens a group at a fractional place, which C3 removed for drawings).
- **Layers that end at the box are cut once; overflow shows less than a pixel.** At a fractional box origin the group's raster is a pixel wider than the
  box. Cutting the box again in the outer warp counts the edge pixel's coverage twice for a layer that ends at the box (picture or fill: 0.36
  where 0.6 is right, found by the first placed test), so the outer warp does not cut when the whole box is shown (`ends_at_window`, as a drawing's)
  and a layer that overflows the box leaks into the pixel its edge falls in. *Alternative weighed:* a coverage clip as the last step of the inner
  render, cut once for overflowing layers and twice for the others; no cut exists that is right for both without knowing which layers end at the box.
- **Cost belongs to the source.** `Source::budget()` (default one layer; a group: one layer plus its scene's count, one deeper than its
  scene), read by `Layer` at creation and by `reference::resolved`, which used to start every layer from one layer and would have dropped a group's
  cost at `add`. A reference to a group inherits it, so `MAX_LAYERS` counts the group again. The scene keeps its count and depth, so a check is a sum.
- **Depth is a stack bound and is measured for the mix.** Each scene is one level (render, lay, land, a boxed closure, render), and a group used
  as a mask layer is two. Mixes to 16 (6 references, 4 scenes, 3 scenes as masks) are drawn, printed, cloned and dropped in 256 KB in the test
  profile, unoptimised (overflows between 64 and 128 KB) and release (runs at 64 KB). `Group` prints its canvas, count and depth, as `Mask` does, because a
  derived `Debug` would print the nest.
- **The 2^27 bound is on the sum of the groups' pictures**, checked in the group before it allocates, and the output picture and each layer's own raster
  keep theirs. The peak measured is 16 bytes a pixel of the groups touched at once (192 MB a level for 12 megapixels), which is the sum bound's
  2 GiB at the limit; a group's buffer is not touched until its first layer is drawn, so a nest of single-layer scenes does not grow the peak at all.
- **A look's `px` inside an inner scene are the inner canvas's design pixels.** The inner layers are drawn on a target whose scale is the group's drawn
  density, so a look run by C5 at the design scale sees the badge's own pixels: it looks the same at every use and every size.
- **Cycles cannot go through scenes.** A scene is an immutable value held by value, so no scene can contain itself; names never cross a group (the
  walks and the cycle check run per scene).
- **What C4c leaves hard** ("Needs from others"): C6 must rebuild a scene's `Arc` when a value under it changes, and address layers of an inner scene; C5
  must give a group a look on its finished raster; C7's cache key of a group is the scene's value, the density per axis and the phase.

## C5a: looks on every layer (2026-11-14)

- **A look belongs to the layer, not to its source.** Looks and the masks by a picture's own maps are a `Finish` in the layer body
  (`finish.rs`); `Opened::draw` runs it on what the source drew, so no source repeats the rule and a reference adds to a copy of
  the named body's. The source only draws, at the size a look runs at, and hands over the maps a look or a mask uses (`Drawn::maps`);
  `Finish` is told by `is_looked`. *Alternative weighed:* a wrapping source that holds the looks (`Looked(inner)`): the maps and
  the masks of the picture would then reach the wrapper through a second interface.
- **A layer with no look keeps its path, bit for bit.** A picture with no look hands over its own pixels (masked by a map through the
  same `Finish::masking`), so source, `fit` and `place` stay one warp; a look makes every source draw at the drawn size first. 45
  scenes (pictures with and without looks, maps and masks, drawings, text, fills, groups, references, perspective, blends, light)
  render bit for bit the same before the refactor, after it and after the new kinds; the benchmarks of C2a to C4c did not move.
- **The adapter file keeps the rule.** `Look` (image's recipe and its render) stays in `source/picture.rs`, the one file that names
  image; `finish.rs` names no sibling crate.
- **A fill with a look is a raster of the layer's size.** A flat colour draws one pixel at any scale; a look needs the size the layer
  has (a scanline or a weave depends on place), so the fill is made that size, bounded by the same 2^27 rule as any layer.
- **Image sees straight colour, the alpha is the source's.** As in C1 for pictures: a drawing's clear corner stays clear, a half
  transparent fill keeps 0.5. A look that makes colour where the source has none (a blur's bleed past an edge) shows nowhere; light
  that spills is C5b (room).
- **What C5b to C7 inherit.** C5b: the look runs in `Finish::run`, which has the `Request` (scale, factor, window) and the drawn
  raster, so the design scale and room are one change there; the raster ends at the source's `covers`. C6: one list of looks and one
  of map masks a layer, whatever the source. C7: a looked layer is drawn at the drawn size and looked again for each use (a reference
  with an unchanged density could share the raster).

## C5b-lite: the design scale from the canvas (2026-11-14)

- **A look's `px` are pixels of the canvas, handed as `Options::with_canvas`.** `Finish::run` tells image the scene's canvas in the
  output pixels the layer is drawn at (`Request::canvas` times the density without the fit: `output scale x place.scale`), and image
  takes the longer side over 480. *Reason:* one `px` is the same number of output pixels on a small inset and a full-bleed photo,
  and doubles with `place.scale`; a group's inner layers get the group's drawn density for free (its inner target's scale).
  *Cost:* image takes whole pixels, so a fractional scale uses the rounded canvas (half a pixel of the long side).
- **A layer larger than the canvas is legal,** so image relaxed its refusal of a canvas smaller than the picture (only a side of 0 is
  refused); the C5a test of a drawing at `scale` 2 now expects the doubled canvas, as it did the doubled picture.
- **Room and light that spill** were built next, in C5c (below).

## C5c: room for light layers (2026-11-14)

- **The room is image's declared spread, summed.** `Look::room` adds `Recipe::spread(presets, longest side)` of each look of the
  layer (a second look works on what the first made, so its halo starts where the first ends) at the canvas long side `Finish::run`
  already hands image, rounds up with `Spread::pixels` and gives the sides top, right, bottom, left. *Reason:* image owns what its
  effects paint and holds each to its word by a test; scene keeps no table of effects. A look image says is `Unbounded`, or a
  layer that would pass image's 2^26 pixels grown, gets no room and **no error**: a render that ran must still run. A layer
  larger than that was never looked at (image refuses it), so the limit is the one image has.
- **Only a light layer has room.** A layer that is not light keeps its source's alpha, so room could only be transparent and
  would be cut; and a look run on it with black room is not the look it had: a blur pulls the edge of an opaque picture dark
  (image's own edge rule keeps the picture's colour). The C5a to C5b look tests, which compare a non-light layer with image's
  render of its own picture bit for bit, failed with black room round it (four of them, by up to 0.11). *Consequence:* a layer
  that is not light is as it was, bit for bit, and the plan's "tiny difference at the edge" is not made. A light layer's look
  sees the colour over black, so black room is the picture it already had.
- **The grown raster is kept.** `Drawn.room` (pixels) tells `Layout::warp` to let the window grown by it through; a raster that
  ended at the window now ends at the window grown (`ends_at_window`), so nothing cuts the halo. Masks by a layer multiply where
  the grown raster lands (canvas space); a mask by the layer's own map is continued by its nearest edge pixel (the rule a blur of
  a map has). The cached raster is the grown one: the room follows from the look documents and the canvas, both in the key.
- **A step that places itself by its picture is placed by the box (2026-11-15).** A look that mixes a halo with an effect that places
  itself by the picture's bounds (vignette, `x`/`y` centres, noise numbered by place, whole-picture statistics) used to see the grown
  raster as the whole, so a vignette's corners were darkened less. The grown raster is now run with image's `Options::with_room`: those
  steps run on the box and keep their pixels, every other step runs on the grown raster and paints its halo into the room. A test
  pins it against image's own render of the box with room (and against the same render without, which differs). **Limit (image's):** a
  boxed step does not reach a halo an earlier step painted into the room (a vignette after a bloom leaves the halo undarkened): order
  the halo steps last. **A layer that is not light still has no room:** its alpha is its source's, so a halo past the box would have
  no alpha to show it, whatever image paints there (image returns colour; a look that returns its stroke as alpha is not built).
- **The "bloom tints the whole box" report was the halo cut at the frame.** A transparent pixel is black to a light layer's look
  and the bloom of black is black; a sun on a frame larger than itself showed a lit rectangle because the halo, wider than the
  frame, stopped at its edge. With room the halo goes on past the frame; `a_halo_wider_than_the_frame_goes_on_past_the_frame_edge`
  compares it with image's render on a frame as large as the halo needs.
- **Oracle.** The layer equals image's own render of the content on a frame grown by hand by the declared room (to 1e-6, the
  rounding of the light's division and product), for a drawing, a picture, a group and a fill; a frame grown by more than the room
  has no light beyond it (below 1e-5, the Fourier blur's rounding); looks add. A hand-padded layer is not the oracle: it is a layer
  with looks, so it gets room too.

## C6a: parameters, one listing per layer (2026-11-14)

- **A value is set only at an address the layer lists.** `Layer::parameters` writes the addresses and `Layer::with_values`
  refuses any address that list lacks, so the two cannot disagree and an unknown address needs no parser: the message
  is the layer's own list and the nearest by the one search `blend` uses (`suggest.rs`). The grammar is written in one file
  (`parameters.rs`); a source says its own fields (`picture`, `map.<name>`, `text`, `color`, `drawing.<param>`,
  `scene.<address>`), a sibling's address stays verbatim after its head. Reason: a second reader of addresses is a second
  grammar, and the round trip (every listed parameter set to its own value) is the cheap proof that one list serves both.
- **Values live on the layers as they were added, and the scene is built again through `add`.** A reference is resolved from
  the changed layers, so a change under a named layer reaches its references, the budgets and the names are checked again, and
  nothing resolved is patched. The price is a rebuild of the stack: 0.34 ms for 1,000 layers, 72 microseconds for a mill-sized frame
  ("What is built", Parameters), under a thousandth of a render.
- **Siblings keep their own checks.** Image's `Recipe::with_values` takes the settings of one look as a set; vector's Params are
  checked by `Param::accept` and held beside the document (vector's own `with_values` fixes a Param and removes it from the
  list, which would make a set value vanish from `parameters()`). A text is made again by vector when its words or colour change.
- **What is not listed** is stated where it is built ("What is built", Parameter limits): a layer under `perspective` has no
  placement field, a map mask has no field, a mask layer has no blend or light. Each range holds only valid values of that
  number alone, so a slider built from `parameters()` cannot ask for an error by itself.
- **What C6b to C7 inherit.** C6b: `Parameter` holds the value a saved scene needs for every address, and a picture's value is
  its file or its array (an array has no file: `to_json` refuses it); `==` is not built. C7: values are on the layers and a layer
  nothing was set on shares its source, so a cache key by layer value sees what did not change; looks are cloned with their
  recipe at each draw (`Finish::clone`), which a cache would avoid.

## C6b: the scene as JSON, and `==` (2026-11-14)

- **One module owns the document.** `document.rs` writes, reads and compares it; a source says only its own keys
  (`Source::document`: `picture` and `maps`, `drawing` and `values`, `text` and its font and options, `fill`, `scene`), so the two
  adapter files still hold all that is known of image's recipe and vector's document (a look is `{"format", "recipe"}`, a
  drawing vector's canonical text). Reader and writer are one file because they are one grammar: the table of keys, the
  defaults and the order are written once.
- **The reader builds a scene by the public calls.** `from_json` calls `Layer::fill`, `fit`, `place`, `mask`, `look`, `add`...
  in the order a person would, so every rule a document must meet is the rule of the call, in its words; the reader adds
  only the shape (a key, a kind, a pointer). A refusal of a call is the `FormatError` at the pointer of the member the error's
  address names (`/layers/0/place/dx`), else of the object. Reason: a second set of rules for documents would drift; none can.
- **Equality is the same document.** `==` is `layer_document(.., Form::Compare)` equal: the document that is saved, with a
  held picture or map as the digest of its pixels (it cannot be saved: it has no file) and a file as where it is. One
  definition for "saved" and "equal" (what is saved is what is compared), no walk of a second tree, no render. The cost is
  that a held picture is hashed at each `==`. A scene is equal to its reading back, which is the property the batch needs.
- **A file is one value.** `Located` is the path as written, the folder it is relative to and the digest it was saved with:
  `to_json` writes the path as written and the digest of the file now, `from_json` keeps path, folder and digest, the render
  resolves the path and checks the digest. So a scene read with a base writes the same relative path again, and a scene
  made by calls writes the path it was given. `to_json` takes no folder: the writer does not know where the text goes.
- **Holes save.** A template is a scene (`cover_template`), so a hole is `null`; a drawing's set Params are `values` because
  vector keeps them outside its document (C6a); the masks are one list in call order because `mask#2` names the second call,
  of either kind.
- **A hostile document is refused at the cheapest place.** The parser is `serde_json` through a visitor that refuses a key
  written twice (an object read as a map keeps the last one silently), at its depth bound of 128; the count of layers
  is checked before one is built; a scene or a mask layer nested deeper than `MAX_DEPTH` is refused when entered, before the
  reader's own recursion could grow (a nest of 40 scenes overflowed 256 KB before this bound, then reads in a fraction of it).
- **New dependencies.** `sha2` (the digest of a file; the SHA-256 the ecosystem's formats use; already in the tree through
  vector), `serde` (the visitor), and `serde_json`'s `float_roundtrip` (without it a placement reads back one ulp away
  from the number written: a property test found `905.1321946557415` read as `...416`).
- **What C7 inherits.** A layer's document under `Form::Compare` is a value a cache key can be made of (a file is its path:
  the cache keys a file by path, size and time as `api.md` says, and the digest is checked at the read). Hashing a held
  picture at each key would be wasteful: C7 hashes once, where the layer is made. The pixels are hashed at `==` only.

## O1 and P1: a PNG through image's writer, and a thin Rust command line (2026-11-14)

**PNG.** `Raster::write(path)` goes through image's `write`, from `source/picture.rs` (the one file that names image). Image's
writer is path-only, picks the format by the extension, rounds a level as `level * 255 + 0.5` cut to a whole number (a level read
from an 8-bit file comes back as it was) and embeds a small sRGB profile with a fixed date, so the same picture gives the same
bytes. *Deviation from the batch's "no ICC":* the profile stays, because the only other way is to change image, a pin bump for no
gain: the values are sRGB either way. *Alternative weighed:* `to_png() -> Vec<u8>`, which image's writer cannot give without a
temporary file. A file the call cannot write is `Error::Write`, a new variant because `Read` says the opposite; it has no Python
class, since no Python call writes (the mapping's last arm turns a variant without one into a `RuntimeError` a test would see).

**The command line** is a library crate (`crates/cli`, `bullterrier-scene-cli`) whose `main(args, version, out, err)` the Python
binding exports, as image's does: tests run it in-process and read the streams, the console script is `sys.exit(run(argv))`.
`clap` is its one dependency (image's own line, which `cargo deny` already passes). The CLI holds no logic of the core: `--set`
looks the address up in `Scene::parameters` only to read VALUE as that kind (a number, a bool, text, a path); an address the
scene does not have goes to `with_values` as text, whose refusal lists the nearest. `parameters --json` is written in the CLI as
the Python package's `Parameter.to_dict()`; `tests/test_cli.py` compares the two, so a second definition cannot drift unseen.
*`validate` renders at size 1* instead of a new walk of every file a scene names: the walk would be a second copy of what a render
checks (files, digests, holes, map sizes, looks the sibling refuses), and one pixel costs a read of each file and little else. It
writes nothing. The cost: a frame within a pixel of vector's limit, which is refused below density 1 (`api.md`, "Drawings"), is
refused by `validate` too.

## PDF output and print colour (2026-11-14)

**A raster PDF, at a physical size.** `Raster::write_pdf(path, &PdfOptions)`: the picture is one image XObject (8 bits, Flate)
on a page of `width / dpi` by `height / dpi` inches. The dpi is the caller's; the scene canvas is the trim size and there is no
paper-size option (the canvas and `--size` already say the picture; a paper size would be a second way to say the same pair).
*Alternative weighed:* keep vectors and text as PDF paths, which needs vector to write PDF and the layer stack (blend modes, looks,
masks) to be expressible in PDF: not planned. The PDF is a delivery of the finished raster, the print counterpart of the PNG.

**Crates.** `pdf-writer` (typst; MIT or Apache-2.0; four small dependencies: bitflags, itoa, memchr, ryu) writes the
objects and is the only PDF code in the tree; it writes no Info, date or ID, so the same picture gives the same bytes. *Alternative
weighed:* `printpdf`, which is a document builder with fonts, layers and an `image` dependency for decoding: more than one
embedded image needs, and a second picture decoder. `miniz_oxide` (MIT, Zlib or Apache-2.0) is the Flate encoder: `pdf-writer` does
not compress, and the crate was already in the tree through `png`. No predictor: the image is a few times larger than a PNG's
filtered one, for a simpler writer. `moxcms` (BSD-3-Clause or Apache-2.0) is the CMYK transform; it was in the tree through
`image`. The tests read the PDF with `lopdf` (MIT, no default features; a dev-dependency), independent of the writer. `pdf.rs` is the
one file that names `pdf_writer` and `moxcms`; it needs no `image`.

**Bleed.** A scene has no content beyond its canvas, so the bleed is the rendered edge pixels repeated outward, in whole pixels
rounded up, with the image clipped to the `BleedBox` (otherwise the overhang of up to a pixel lands in the crop-mark margin). The
`BleedBox` is exactly the requested millimetres. *Alternative weighed:* render a larger canvas for the bleed, which the caller can
do by drawing the scene bigger than the trim and giving the page the trim's `TrimBox`: not built, since a layer fitted to the
canvas does not extend past it anyway. The honest limit is stated in `api.md`: a detail at the edge is stretched. The count of
pixels is rounded to a millionth of a pixel before it is rounded up, because 0.2 mm at 254 dpi is 2.0000000000000004 pixels in
doubles and would otherwise bleed three. **Crop marks** are vector paths in `DeviceGray`: 0.25 pt wide (Adobe's default), 5 mm
long, starting 3 mm outside the bleed (so none touches it); named constants in `pdf.rs`.

**Options and ranges.** `dpi` 1 to 2400 (2400 is the finest imagesetter resolution) and `bleed_mm` 0 to 50 are checked where they
are given, with the other limits at the write: the page may be at most 14,400 pt (PDF 1.7, Annex C), and the image with its bleed
at most 2^27 pixels, the render's own limit (`MAX_PIXELS`, one place). No lower page limit is enforced (Annex C says 3 pt; a one
pixel picture at 300 dpi is 0.24 pt), because a tiny page is the caller's picture. Transparency is a soft mask only when some pixel
is not opaque; `flatten` over white is the one explicit alternative (a colour to flatten over is not offered: paper is white).
Python has no `render_pdf` for the reason `render_png` is absent; the console script is the route.

**CMYK: built, through the caller's profile.** The question was whether `moxcms` converts RGB to an arbitrary CMYK profile, many
of which are table based. Evidence (`moxcms` 0.8.1): its `lut_transforms` has an RGB-or-Lab to CMYK branch that builds a 33-point
table from the source and the destination's PCS-to-device (`B2A`) table, for the lut8, lut16 and `mAB` types, with 8-bit output as
four channels; a spike converting sRGB through macOS's Generic CMYK profile (a table profile) gave white (1, 1, 0, 0), black
(188, 180, 164, 222), red (1, 214, 240, 3), mid grey (131, 109, 100, 5), green (106, 0, 154, 0), blue (225, 173, 0, 0): plausible
for a perceptual conversion with no black point compensation. No profile is bundled (licences, and the profile is the print
shop's); the file is read when the PDF is written and embedded unchanged as `ICCBased` with `/N 4`. The test builds a CMYK profile
by hand (a `mft2` B2A table affine in Lab, so interpolation reproduces it) and checks white, black and red against the lightness
worked out from the sRGB primaries, within 2 levels. **Not done:** a rendering intent or black point choice (perceptual is
`moxcms`'s default), an ink limit, soft proofing, CMYK for the PNG. The conversion runs before the bleed repeats the edge, so
the bleed is the converted edge.

## Smaller choices of the API (2026-11-13)

- **A group is a scene, a mask is a layer.** No group type and no mask type; one `Layer` does both.
- **One `Layer.picture`.** A picture of `None` is a hole the caller fills, so footage needs no second kind of layer.
- **Gradients are vector's.** A rectangle filled with `btvec.linear_gradient`; scene has no second gradient.
- **`Path` is a file, `str` is text or a name.** A mistaken `str` is a `TypeError` naming the fix, not a guess.
- **A look that sets `size` or `frame` is refused.** Scene owns a layer's size and fit; regions are fine.
- **Equality is the same document** (C6b, above). Equal scenes with equal values render equal pictures.

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

## Banded rendering (B1) (2026-11-14)

- **One plane, a window of rows.** A render draws every layer for one plane (the canvas, or the canvas and a margin for a mask whose
  blur reaches past it). `Target::window` is the rows of the plane that are drawn; the whole render is the window of all of them, one
  band, through the same entry (`render::draw`), so there is no second path to keep equal to the first. The warp is told which rows
  it is to give (`warp::Output`) and computes each from the map of the whole plane, so a pixel is what it is whichever rows are drawn
  with it. *Alternative weighed:* the same map shifted by the band's first row (as a mask's margin is): the inverse of a shifted map
  is another set of floats, and a pixel would differ in its last bit. *The oracle:* for every example scene at heights 1, 7, a prime
  near an eighth, half, all and a height that divides nothing, at sizes whose long side is odd and even, the bands equal `render`
  bit for bit (`tests/it/bands.rs`, `tests/test_bands.py`). A wrong band offset in the band's own window fails 7 of the 9 Rust tests; but
  an error in code the whole render shares moves both alike, so `render` was also compared with the render of `main` before the change
  (the 15-layer A3 poster below at three sizes, the cache tests' poster, the scene of every kind of parameter, nine digests of the
  floats): identical.
  *Hand mutants* (each breaks the code on purpose, the band tests run): the band's first row one too low, 8 tests fail; the margin of a
  mask's band dropped, 3; the last band one row short, 7; the sink called in the wrong order, 7; no room round the rows a group is
  drawn for, 1; a plugin kind not kept and so drawn again for each band, 1 (it is asked once): all killed. **One survives:** the
  boxes of the blur not aligned to the whole plane. Unaligned, the sums of a pass are added in another order and differ in the last
  bit of an `f64`, which the cast to `f32` hides except for a value within 1e-9 of a rounding step, so no test over a small plane can
  tell. The alignment is kept because it is what makes the bands equal the whole by construction, not by luck of a rounding.
- **What is drawn by band, and what whole (the floor).** By band: a picture with no look (the warp reads the rows it needs from the
  picture's own pixels), a flat colour (one pixel), a group with no look (drawn for the rows of its raster the band reads:
  `warp::rows_read` takes the four corners of the band through the map back, with the room the kernels read, and `View` lets the warp
  read a part of a raster as some rows of a taller one), a mask plane (the band and the reach of its blur). Whole, once, kept in the
  cache and cut per band: **a layer with a look that cannot be banded** (below), and **a plugin kind that does not say it draws by position**. *A drawing is windowed
  (2026-11-15):* vector's window render is the crop of its whole render byte for byte, so a drawing is drawn for the rows a band reads,
  and a refused window (a side past 65,000 pixels, 2^27 in all) is a raster the whole render refuses too: no whole fallback. *A kind is asked for a band's rows only if it says so (2026-11-15):* its pixels are promised by rectangle and
  scale, not by position (a kind with a seeded stream draws other pixels for another rectangle), so `LayerKind::by_position` is false
  by default. A kind that returns true is asked for the rectangle the rows divide its window into (`rows_area`: the same pitch, the
  rows' share of the height), and `check_by_position` (public, for the kind's own tests) holds it to "the rows of the whole are what
  it draws for those rows" bit for bit. Python: `register_layer(kind, draw, by_position=True)`. A group's inner looked layers are whole
  at the group's scale and kept, so each band of a group finds them. State of the floor: the looked layers' rasters
  stay resident (the cache holds up to 2 GiB of them).
- **Banded looks (2026-11-15).** A looked layer whose source draws rows (a drawing, a group) is drawn for the rows a band reads and
  the margin its looks reach each side (`Finish::band_margin`: the sum of image's `Recipe::band_margin` of each look), and each look
  runs on those rows through `Options::with_window(first_row, raster width, raster rows)`, so a vignette's place and noise numbered by
  position are the whole raster's. The window's canvas is the raster, and the canvas also sets a look's design scale, so the layer is
  banded only where the raster's longest side is the scene canvas's (a layer fitted to the canvas; a smaller or a zoomed one is drawn
  whole). Run whole too: a `light` layer (its looks run on a raster grown by room), a mask by a map, a recipe image cannot band
  (whole-picture statistics, `each`, driven steps, maps, a `frame` or `size`), and looks whose margins add up to 16 pixels or more:
  image sums a blur of that reach by a Fourier transform whose rounding depends on the rows it is given, so a band differs from the
  whole in the last bits (measured: the bit-exact oracle fails with the limit lifted). Revisit with image's exact-only
  `band_margin` (its roadmap). A looked picture is resized to the drawn size for the rows wanted
  (`warp::resize_rows`: the same warp with an output of some rows, each pixel the whole resize's), then looked; a picture shrunk past
  2x is averaged whole for each band until the reduced raster is kept (`future.md`). A look or a mask that reads a map the source
  holds runs whole.
- **A picture shrunk past 2x is averaged once, and a band's plane is expendable (2026-11-15).** The warp averages a raster down first
  where its map shrinks it past 2x and its footprint is a rectangle along the axes; each band did it again. A band now lands from the
  average, kept in the cache under the source, its looks and masks, the request and the size (`warp::reduction`, `reduced_map`); a pixel
  is what the whole render makes (the oracle). The plane of a mask drawn for a band is read by that band alone: it is put in the cache
  as expendable, so it is kept only where it fits without costing a finished layer, and leaves first, where it used to evict the looked
  layers of the render.
- **The zoom limit.** A layer that draws only its own pixels with no look (a picture's, a flat colour's one) allocates nothing of the
  size it is placed at, so its drawn size is no longer limited to 2^27 pixels: a side of 2^31 is (what the warp's positions keep
  exact). A group drawn by band is limited by the rows it draws, so a group zoomed a thousand times (100,000 by 100,000 pixels) is
  drawn. A look, a drawing and a plugin kind keep the 2^27 drawn-size limit (their raster is the size they are drawn at), and so does
  `render`'s output; `render_bands` and the file writers limit what is held (a band is at most 2^27 pixels) and the long side
  (65,000, vector's side and the size's own limit). A perspective layer is still cut down to 2^27 pixels as before (C2b).
- **The band height is a memory budget, stated once.** `BAND_PIXELS` = 2^24 (an eighth of 2^27): a band of float32 RGBA is 256 MiB,
  its height is `2^24 / columns` rows (at least one). Not a knob: `render_bands` takes the height as an argument for tests and API
  users, `render_to_png` and `render_to_pdf` take none. The command line writes a picture of at most 2^24 pixels as before and streams a
  larger one: the same constant, so one band is the whole render.
- **PNG: the `png` crate, streaming, the `sRGB` chunk.** Image's writer is whole-image (it builds the PNG in memory), so a picture of
  139 megapixels cannot go through it. The `png` crate (MIT or Apache-2.0, already in the tree through image) has a streaming
  encoder: `render_to_png` writes each band as it is made, 8 bits a channel rounded as image rounds them (`level * 255 + 0.5`), filter
  adaptive, compression balanced (`Best` is several times slower on a print). Image embeds a 588-byte ICC profile it keeps private; the
  streamed file says sRGB by the PNG `sRGB` chunk (perceptual intent), which every viewer reads and which needs no profile bytes. The
  pixels are those of `Raster::write` once decoded (a test over a picture of two bands); the bytes are not.
- **PDF: one Flate stream, written incrementally.** The writer takes rows as they come (bleed rows repeat the first and last
  row, a CMYK profile converts each row as before, the alpha stream is kept only if some pixel was not opaque) and compresses them
  through miniz's streaming deflater into two buffers; `Raster::write_pdf` is one push of the whole picture, so the two share one
  path and the boxes, the image and the operations are the same (a test of a picture of two bands with bleed and crop marks decodes both
  and compares). **The floor of a PDF is its compressed image** (held until the file is assembled, as pdf-writer writes whole objects),
  not the picture or its 8-bit planes; the 2^27 pixel limit of `write_pdf` is lifted for `render_to_pdf` (the page's 14,400 pt and the
  PDF's 32-bit sides are the limits). A file is written under a temporary name and renamed when whole: a render that fails leaves no file
  and a file that was there stays.
- **Storage precision (decision 2): float32.** A band is float32 premultiplied RGBA, 16 bytes a pixel, and no finished layer is stored
  but the looked ones (whole, float32, in the cache). *Why not 8 or 16 bits:* image keeps float32 for working pictures (8 bits bands in a
  smooth gradient and cannot take a further grade); 8 bits would change the pixels of a render, so the bands could not equal what
  `render` makes today; and what 8 bits would save is not what the peak is made of (below). *Measured* (release build, load average 5
  to 20, peak resident set of the process, the scene's inputs included; the scene is a stand-in for the C7 poster, whose builder was not
  kept: 15 layers on 4,961 by 3,508 design pixels at size 4,961: a ground, a 2,400 by 1,600 photo covering the canvas, a rotated photo
  at 0.9, a looked photo, a photo masked by its own feathered map, a multiplied band, a drawing, two texts, a group put at 2x and 10
  degrees, a looked reference to the looked photo, a feathered mask layer by a drawing, a looked light fill, a picture in perspective, an
  overlay fill): **whole 1.47 GB, 1.66 s the first render and 0.55 s the second; banded in 1,000 rows 0.89 GB, 1.87 s; `render` on `main`
  1.47 GB, 1.72 to 2.87 s, on this branch 1.47 GB, 1.68 to 2.26 s** (three runs of each, interleaved: no regression, the times are the
  machine's). The band is 79 MB there; what is left is the inputs, the looked layers and the cache. **A0 at 300 dpi** (9,933 by 14,043
  pixels, 139 megapixels, over 2^27: `render` refuses it): eight layers (a group of the full canvas size, two photos of 12 and 6
  megapixels, one rotated, a multiplied band, type at 1,600, a feathered mask layer, an overlay fill) written to a PNG by `render_to_png`:
  **25 s, 1.77 GB peak, a 44 MB file** that decodes to 9,933 by 14,043. A band there is 1,689 rows, 270 MB; the group alone would be
  2.2 GB whole.
- **Not built, and why.** A PDF whose compressed image is not held (a decision: the two compressed streams go to temporary files and a
  hand-written container assembles them, or pdf-writer takes a stream; `future.md`); banded looks of a recipe image cannot band, of a
  blur summed by a Fourier transform and of a layer that is not the canvas's size (image's exact-only `band_margin`, and a window with a
  canvas apart from the scale; "Banded looks" above).
- **Measured 2026-11-15 (a stand-in A3 poster of 15 layers, a looked photo of vignette and grain, one bloom, a perspective picture;
  a debug-info test build, one run each, the machine's other load unknown), before and after banded looks, the drawing, kind and
  room batches:** whole 4.33 GB and 14.2 s the first render, 1.40 s the second (after: 4.33 GB, 15.2 s, 1.38 s); banded in 1,000
  rows **3.25 GB, 14.9 s before, 2.96 GB, 14.8 s after** (the vignette and grain of the one banded look; the bloom layers are whole).
  An A0 print at 300 dpi of eight layers to a PNG: 31.7 s, 1.66 GB before, 31.9 s, 1.68 GB after (nothing there is looked). The
  stand-in is heavier than the earlier one (a bloom at the canvas's size), so the figures above are not comparable with it.
