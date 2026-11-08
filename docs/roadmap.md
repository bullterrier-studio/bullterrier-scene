# Roadmap

Status: draft for review, 2026-11-08. What the batches build: [design.md](design.md).

Small batches, in order. Each gets its own plan, ends merged into `main` with the whole gate green and, for M and L batches, an
independent review done. Sizes: **S** one working session, **M** two or three, **L** four or more.

## When work starts

Owner's decision, 2026-11-08: no work on scene starts until image's E4 and vector's V10 are finished, merged and pushed. After
that, each batch starts when what it needs exists (the last column of the table). Scene pins image and vector at a revision, so
their later changes cannot break it; a bump of the pin is a reviewed commit, and the adapter files (`source/picture.rs`,
`source/drawing.rs`) take the change.

| Batch | Starts when |
|---|---|
| A1 | image E4 and vector V10 are merged and pushed (A1 is docs only; it could start earlier, the owner chose not to) |
| C1 | A1 is reviewed; C1 uses only the public APIs of image and vector as they are then, so it does not wait for image E5 to E7 |
| C2, C3, C4 | C1 is merged |
| C5 | image has effect footprints and the owner has decided on a design scale from the caller |
| M1 | C3, C4 and vector V10b are merged |
| B1 | image has a banded render (after its E6) and position-hashed noise, and vector has V10c (window render); until then seeded-noise layers render whole |

## How we work

1. **Plan:** one page per batch in the git-ignored `.superpowers/plans/`: the settled edge cases and the task list.
2. **Branch:** one short-lived branch from `main`, merged and deleted. No pushes unless the owner asks.
3. **Build:** one implementer per batch, test first (see it fail, then pass), committing after every green step. While working,
   run only the tests near the change; `uv run poe check` once before the merge.
4. **Mutation checks:** about ten hand mutants per batch, on the settled edge cases and the arithmetic (a sign, a boundary, an
   ignored argument). A survivor on core behaviour becomes a test.
5. **Review:** M and L batches get one independent review of the whole branch; the reviewer proves each finding (a failing test
   or a reproduction) and a finding it cannot prove is dropped.
6. **Reports:** commits, public signatures, deviations from the plan, mutants killed or survived.
7. **Every batch ends with something to look at:** an example scene and the picture it renders, under `docs/gallery/`, as image
   and vector do for an effect. From C1 a `look` task renders an example scene to a git-ignored folder and writes the PNG to
   open. A batch without such a piece is not done.

## Testing

Behaviour through `Scene`'s public API, expected values from independent oracles, never the code's own formula:

- blend modes against hand-computed tables from the W3C formulas;
- placement: a round trip through the inverse transform returns the point; a half-pixel shift equals the average of its
  neighbours;
- an opaque normal-blend layer on top equals that layer; a fully transparent layer changes nothing;
- an inverted mask is the complement of the mask;
- a banded render equals the whole render (metamorphic), and the cached render equals a cold one;
- known-good and known-bad scenes for every checker; no golden images.

Real objects over mocks: the sibling crates are real, used through their public APIs.

## Batches

| # | Batch | Size | Needs from image or vector |
|---|---|---|---|
| S0 | Scaffold, gates, README, design and roadmap | S | |
| A1 | The code API, designed from example programs first (a poster, a cover, a template run with a photo and a title, a video-frame use), `docs/api.md`, reviewed | M | |
| C1 | Walking skeleton: `Scene` with a canvas and layers from a picture, a picture with an image look and a vector drawing; normal blend; whole render to RGBA in Rust and Python; replaces the template's `scale` | M | their public APIs as they are; git dependencies pinned |
| C2 | Placement: position, rotation, scale, skew, perspective, anchor, fit; resampling | L | |
| C3 | Masks: from a drawing's shape or text, an alpha, luminance, a given map, a subject cut-out; invert and feather | M | image's subject finder |
| C4 | Blend modes (the W3C separable and non-separable set) and groups (isolation, group opacity) | L | |
| C5 | Effect chains on layers and groups; the design-scale convention; room around a layer | M | image: a design scale from the caller (owner decision pending), effect footprints |
| C6 | Parameters in the shared description format; the scene as JSON | M | |
| C7 | Caching per layer: change one value, redo only what depends on it | M | |
| M1 | First milestone piece: a two-ink geometric poster with a photo clipped into a polygon, type in front of and behind a cut-out, flat shapes and grain | M | vector V10b (gradients, blend set, clips) |
| B1 | Bands: compose band by band, finished layers compact, a 15-layer A3 poster measured whole and banded | L | image: footprint, banded render, storage precision, position-hashed noise so seeded effects can band; vector: V10c window render |
| O1 | Output: PNG, then PDF; print colour (CMYK, ICC, bleed) when wanted | M | |
| P1 | Command line `bt-scene render`; templates and presets | S | |
| R1 | First release | S | image and vector published first; the repository opens up |

## What each batch shows

| Batch | What you can open and look at |
|---|---|
| A1 | `docs/api.md` and the example programs, to review; nothing renders yet |
| C1 | the first picture: a photo with a vector headline over it and one image effect on the photo |
| C2 | rotated, skewed and perspective-tilted photos stacked on a canvas, like a pile of prints |
| C3 | a photo clipped into big letters; type tucked behind a cut-out subject. The first piece that looks like a poster |
| C4 | a contact sheet of every blend mode on the same two layers, to compare by eye with the same blends in a browser |
| C5 | grain over the whole poster and a halftone on one photo only; one effect at two layer sizes side by side, equally coarse |
| C6 | the second milestone piece: a scene file and a command that swaps the title and the photo, six covers from one scene |
| C7 | the third milestone piece: a timing table (the first render against changing one value) and a 24-frame sequence of a headline sliding in, as a contact sheet; a clip needs ffmpeg and is optional |
| M1 | the first milestone piece: the two-ink geometric poster at three sizes, the same picture each time |
| B1 | numbers: peak memory of a 15-layer A3 poster, whole against banded, and a check that the pixels match |
| O1, P1 | a PNG and a PDF to print; `bt-scene render scene.json out.png` |

Order: C1 is the first batch that uses image and vector. M1 is the milestone that shows whether the layer model is right and
can start once C4 and vector's V10b exist. B1 waits for image's banded render and vector's window render; until then a scene
renders whole.

## Later, each when something triggers it

- A browser build (WebAssembly), if image's crate builds for it and a page needs it.
- Stamped brush marks along a path (spray, scribbles) as a layer type.
- Footage layers, with video.
- Linear-light blending as an option.
