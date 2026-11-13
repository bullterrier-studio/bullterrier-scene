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
a layer used twice has its parameters twice (see `docs/api.md`, Open questions).

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
