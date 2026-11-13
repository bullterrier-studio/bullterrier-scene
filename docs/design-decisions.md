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
fixed, and the finished raster is resampled once. A layer is looked at its fitted size, so a look's `px` is a canvas pixel as
the layer is fitted and `place` scales a halo with its layer. *Reason:*
every placement number has an address (a chain of verbs cannot), and every range holds only valid values, so a slider built
from `parameters()` cannot ask for an error by itself (a zero or negative scale, a flip as a negative number; skew is applied
x then y, so its bound of 89 degrees is enough). A rule across two numbers of an embedded look is the sibling's. `fit`'s slack is `align`,
the pivot of `place` is `anchor`: one name per concept. `scale` is one number because it is what an animation drives.

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
