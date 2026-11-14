# Roadmap

Status: draft for review, 2026-11-08; prerequisites and demo findings updated 2026-11-13. What the batches build: [design.md](design.md).

Small batches, in order. Each gets its own plan, ends merged into `main` with the whole gate green and, for M and L batches, an
independent review done. Sizes: **S** one working session, **M** two or three, **L** four or more.

## When work starts

Owner's decision, 2026-11-08: no work on scene starts until image's E4 and vector's V10 are finished, merged and pushed. State on
2026-11-13: **met.** Image finished its Rust port (E1 to E7, with effect footprints and a banded render) on 2026-11-11; vector
finished V10 and V10d (gradients, the full separable blend set, clip groups, soft masks) and V16 (SVG import, trimmed). Not yet
built in vector: V10e (window render, B1 needs it) and V10f (text runs, M1 needs it if mixed fonts are wanted). After the first
batch, each batch starts when what it needs exists (the last column of the table). Scene pins image and vector at a revision, so
their later changes cannot break it; a bump of the pin is a reviewed commit, and the adapter files (`source/picture.rs`,
`source/drawing.rs`) take the change.

| Batch | Starts when |
|---|---|
| A1 | now (image E4 and vector V10 are merged; A1 is docs only). Review delegated by the owner on 2026-11-13 to the advisor and one independent read-only reviewer |
| C1 | Done 2026-11-13 (image `f319f13`, vector `05debff`) |
| C2, C3, C4 | C1 is merged (C3: done 2026-11-13, vector `5442066`) |
| C5 | C1 is merged; image has effect footprints (done, E4); image's frame of reference is settled (decision 1 below), and `Options::with_canvas` exists once C5 needs a layer on a larger canvas |
| C6 | C2, C3 and C4 are merged: the layer model is stable enough to write down as parameters and JSON |
| C7 | C5 and C6 are merged: the cache is keyed on parameter values and layer chains |
| M1 | C3, C4 are merged (vector V10d is done); vector V10f (text runs) too if the piece wants mixed fonts |
| M2 | C2 to C5 are merged (placement, masks, references, per-layer looks); vector has gradients and subtract |
| M3 | C2 to C5 and C7 are merged (the piece needs placement, masks, per-layer effects and the cache) |
| B1 | image has a banded render (done) and `Options::with_canvas`; vector has V10e (window render); the owner has decided the storage precision (decision 2). Until then a layer with a whole-picture effect renders whole |
| O1 | PNG: with C1. PDF and print colour: when the owner wants a print, after C5 |
| P1 | C6 is merged (a scene is data); the command line is Rust (decision 3) |
| R1 | image and vector are released, B1 is done or the owner defers it, and the owner decides to open the repository |

**Before starting any batch:** the previous batch is merged with the gate green; its row above is met; the pin of image and
vector points at a revision that has what the batch needs (a bump is its own commit, with the gate run); the batch has its plan
in `.superpowers/plans/`.

**Decisions the owner still owes**

| # | Decision | Needed by | Where it is recorded |
|---|---|---|---|
| 1 | ~~A design scale handed to effects by the caller~~ **Settled by image, 2026-11-11:** the design scale comes from the canvas (the working picture's long side over 480) and travels in the frame (`Place`); there is no public setter. A public `Options::with_canvas` is a later, additive change "when a caller needs it": scene is that caller, so C5 asks image for it | C5, B1 | image's design decisions, "the frame of reference (banded render, B1)" |
| 2 | Storage precision of finished layers: 8 bit, 16 bit or float32. Image keeps float32 for working pictures (8 bit bands in smooth gradients and cannot take a further grade); still open for layers that wait for the canvas | B1 | image's design decisions |
| 3 | ~~Command line in Rust or Python~~ **Settled in A1, 2026-11-13:** a Rust command line crate that the Python console script runs, as image does | P1 | [design.md](design.md), open question 4 |
| 4 | A browser build of scene | when a page needs it | [design.md](design.md), open question 1 |

## What the demos and the experiment asked of scene

Found 2026-11-12 and 2026-11-13 by rebuilding earlier pieces with the finished libraries (the demos repository's `feedback/gaps.md`,
and the earlier Remotion-style experiment). None is a gap in image, vector, control or audio; each is scene's job. The piece that
shows them all is M3.

| Need | Evidence | Batch |
|---|---|---|
| Move, scale and stand a cut-out subject on a floor | the dog could not leave its photo position (G1) | C2, M2 |
| An effect on one layer only: the logo's glow must not light the windows; the sun glows, the dog does not | a whole-picture `bloom` after a bitmap blend (G2) | C5 |
| Contact shadow and floor reflection of a cut-out | image has `drop_shadow` and `reflection`; nothing ran them on one layer (G3) | M2 |
| Reframing to the delivery shape without an outside crop (777 to 720 wide, 9:16 from a taller photo) | an ffmpeg crop in a script (G4) | C2 (`fit`) |
| One colour typed once, for vector, image and the caller's code | the same hex typed three times (G5) | C6: a scene's own colour is one parameter; inside looks and drawings it waits for a colour reference in image and vector ([api.md](api.md), Open questions) |
| A region of a layer as the target of an effect: the sign, each window as its own light | a hand-made numpy rectangle and a whole-picture bloom (G8) | C3 (a mask from an image region), C5 |
| An effect's space: on the layer's own picture, or on the canvas | the experiment let each effect choose a photo or a screen space | C5: decided in A1, the layer's own picture ([api.md](api.md), "Looks") |
| A static layer renders once: the graded photo is the same in 300 frames, only the bloom amount and the split follow the music | every demo frame redoes every layer | C7: a layer's cache key is its own inputs and parameter values |
| Values per frame arrive as `scene.with_values(...)` and `render(size)` takes none; the mapping from a signal to a value is not scene's | `frames.py` `frame()` held the bindings | C6, with video |
| What scene does not take on: facade structure detection (windows), prompted segmentation, footage mattes, shot cuts, a timeline | the experiment did them inside its renderer | image (a region recipe, an optional finder), video |

## Where the entire work ends

Decided 2026-11-13, with its reasons in [design-decisions.md](design-decisions.md), "Where the entire work ends": A1, C1 to C7,
M1 to M3, O1 (PNG only), P1 and the final poster; B1 is deferred.

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
| A1 | The code API, designed from example programs first (a poster, a cover, a template run with a photo and a title, a video-frame use), `docs/api.md` and `docs/examples/`; review delegated to the advisor and one independent reader | M | |
| C1 | **Done 2026-11-13.** Walking skeleton: `Scene` with a canvas and layers from a picture, a picture with an image look and a vector drawing; normal blend; whole render to RGBA in Rust and Python; replaces the template's `scale`. What is built and where it differs from the contract: [api.md](api.md), "What is built" | M | their public APIs as they are; git dependencies pinned |
| C2 | Placement: `dx`, `dy`, `scale`, `aspect`, `rotate`, `skew`, `flip`, `anchor`, perspective, `fit` (cover and contain produce the delivery crop, such as a 9:16 frame from a taller photo, so no tool crops afterward); references (`Layer.of`); resampling. **C2a done 2026-11-13** (`fit`, the affine `place`, the one warp, the delivery crop; what is built: [api.md](api.md), "What is built"); **C2b done 2026-11-13** (`perspective`: a homography to a convex quadrilateral, mirrored accepted; one footprint rule for every warp: [design-decisions.md](design-decisions.md)); references (`Layer.of`) are not in C2 so far and wait for a plan | L | |
| C3 | **Done 2026-11-13.** Masks: from a drawing's shape or text, an alpha, luminance, a given map, a subject cut-out; invert and feather (three box passes, a margin so an edge feathers right); maps on a picture, handed to its looks; text layers. What is built: [api.md](api.md), "What is built"; what was settled: [design-decisions.md](design-decisions.md) | M | image's subject finder (not used: maps are handed in) |
| C4 | Blend modes (the W3C separable and non-separable set) and groups (isolation, group opacity). **C4a done 2026-11-14** (the sixteen modes, `as_light`, the mask-layer refusals; what is built: [api.md](api.md), "What is built"; what was settled: [design-decisions.md](design-decisions.md)); **C4b** (a scene as a layer, `Layer.of`) waits for a plan | L | |
| C5 | Effect chains on layers and groups; light layers (`as_light`); the design-scale convention; room around a layer | M | image: effect footprints, and `Options::with_canvas` (the design scale is settled by image, decision 1) |
| C6 | Parameters in the shared description format; the scene as JSON; named colours (a palette) that layers, drawings and image effects alike refer to, so one colour is typed once: only for scene's own colours until image and vector take a colour reference | M | image and vector: a colour reference |
| C7 | Caching per layer: change one value, redo only what depends on it; plugin layers (`Layer.make`, `register_layer`) as the test seam that counts `draw` calls | M | |
| M1 | First milestone piece: a two-ink geometric poster with a photo clipped into a polygon, type in front of and behind a cut-out, flat shapes and grain | M | vector V10d (gradients, blend set, clips) |
| M2 | A composed-world piece: a cut-out subject standing in a generated world (vector sky, sun and receding grid floor), moved and scaled, with a contact shadow, a floor reflection and a glow on one layer only | M | image: `rim_light` and `bloom` as looks (the shadow and the reflection are references in scene, no image effect); vector: gradients, subtract |
| M3 | The mill piece rebuilt as one scene (see "What the demos and the experiment asked"): a night-graded photo with windows and a sign glowing as separate layers, a logo drawn on, a title landing, a 9:16 delivery crop, every moving value set per frame; replaces the demos' hand-written frame function | M | image: `bloom`, `gradient_map`, `halftone`, `rgb_split`, regions; vector: `from_svg`, `slice`, text with words |
| B1 | **Deferred** ([design-decisions.md](design-decisions.md), "Where the entire work ends"). Bands: compose band by band, finished layers compact, a 15-layer A3 poster measured whole and banded | L | image: footprint, banded render, storage precision, position-hashed noise, and an origin plus canvas size handed to effects, so that seeded and position-aware effects can band; vector: V10e window render |
| O1 | Output: PNG only, through image's save; PDF and print colour (CMYK, ICC, bleed) wait until the owner wants a print | M | |
| P1 | Command line `bt-scene render`, a Rust crate that the Python console script runs; templates and presets | S | |
| R1 | First release | S | image and vector published first; the repository opens up |

## What each batch shows

| Batch | What you can open and look at |
|---|---|
| A1 | `docs/api.md` and the example programs in `docs/examples/`, to review; nothing renders yet |
| C1 | the first picture: a photo with a vector headline over it and one image effect on the photo |
| C2 | rotated, skewed and perspective-tilted photos stacked on a canvas, like a pile of prints (C2a: turned, skewed and mirrored; C2b: the tilt, and a checker road to a horizon), and a tall photo cut three ways into a 9:16 frame |
| C3 | a photo clipped into big letters; type tucked behind a cut-out subject. The first piece that looks like a poster |
| C4 | a contact sheet of every blend mode on the same two layers, to compare by eye with the same blends in a browser (C4a: `uv run poe look`, `docs/gallery/c4a-blend-sheet.png`) |
| C5 | grain over the whole poster and a halftone on one photo only; one effect at two layer sizes side by side, equally coarse |
| C6 | the second milestone piece: a scene file and a command that swaps the title and the photo, six covers from one scene |
| C7 | the third milestone piece: a timing table (the first render against changing one value) and a 24-frame sequence of a headline sliding in, as a contact sheet; a clip needs ffmpeg and is optional |
| M1 | the first milestone piece: the two-ink geometric poster at three sizes, the same picture each time |
| M2 | a composed-world piece: a bull terrier standing on a neon grid floor under a striped sun, with its shadow and reflection on the floor and glow on the sun only; the same layers as a prototype that had to fake each of them with a hand-made frame loop |
| M3 | the mill piece: `bt-scene render` of one scene at 24 chosen moments as a contact sheet next to the demo's frames, with the glow on the logo only and the windows pulsing on their own; a timing table of the first frame against the next |
| B1 | numbers: peak memory of a 15-layer A3 poster, whole against banded, and a check that the pixels match |
| O1, P1 | a PNG and a PDF to print; `bt-scene render scene.json out.png` |

Order: C1 is the first batch that uses image and vector. M1 is the milestone that shows whether the layer model is right and
can start once C4 and vector's V10d exist. B1 waits for image's banded render and vector's window render; until then a scene
renders whole.

## Later, each when something triggers it

- A browser build (WebAssembly), if image's crate builds for it and a page needs it.
- Stamped brush marks along a path (spray, scribbles) as a layer type.
- Footage layers, with video.
- Linear-light blending as an option.
