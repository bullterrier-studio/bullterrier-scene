# Future: what scene must provide after the first release

Set 2026-11-14. The ecosystem view (three directions, order, library changes in every repo) is in the design repository's
`future.md`. This file is scene's part. Nothing here redesigns scene: each item is built on what exists.

## The three directions, from scene

| Direction | What scene must provide | Status |
|---|---|---|
| Animated posters | `scene.with_values({...}).render(size)` per frame (built with C6a); the per-layer cache C7; a fast path for layers that only move; bands B1 for large frames. Video owns the clock, the signal mapping and the encode | cache and fast path built (C7); bands built (B1: whole-picture looks and drawings are drawn whole, below) |
| Websites | A web export target that reads the scene JSON (C6b, built): each layer an image or a vector, placed with CSS transforms, masks as CSS masks, blends as CSS `mix-blend-mode` (the W3C set by CSS names, on purpose), links as hit areas. Effects are baked into the layer images. Also a render worker (design README, "As a paid web app") | JSON built (C6b); export not planned |
| Drag-and-drop editor | `scene.parameters()` for controls (unit, range, meaning per address); a hit-test call ("which layer is under this point"); an edit API for the layer tree (add, remove, reorder, regroup); a draft at a smaller size (built: `render(size)`); undo as immutable values (built) | parameters in progress; hit-test and tree edit not planned |

**The move-only fast path (built, C7).** `dx`, `dy`, `rotate`, `skew`, `flip`, `aspect`, `opacity` and `blend` only re-warp a kept raster of the layer; `scale` draws the source and runs its look again (a look's pixels follow the density). A drawing, a text and a group drawn at a fraction of a pixel also draw again when that fraction changes (the pixel phase is in their key); measured cheap in the mill (with the other values held, a logo moved by whole pixels takes 0.025 s a frame, by a fraction of a pixel 0.029 s), and the wow loop's dog is a picture, which has no phase.

**Measured with the cache** (release build, 2026-11-14; "Limits, measured" in `api.md` has the conditions): the wow poster's loop 0.506 s a frame before, 0.191 s after; a mill frame with every value changing 0.524 s before, 0.413 s after (and a mill-style frame 1.03 to 0.515 s once a look chain is kept stage by stage: the table in `api.md`); a mill frame that changes one value 0.025 to 0.24 s; a 15-layer A3 poster at 300 dpi 5.2 s the first time and 0.39 s with one headline changed, peak 3.15 GB against 2.16 GB.

**The next speed steps, by what they would save:** (1) *A scan-resistant bound.* A render that scans more than the 2 GiB keeps and reuses nothing; keeping the layers that were the costliest to draw, or the most recent ones of a scan, would make a poster slightly over the bound still fast. (2) *A look that follows the phase less.* A looked drawing that shakes by a fraction of a pixel runs its look again each frame; a look drawn at phase 0 and shifted by the warp would not, and softens by one resample, so it changes renders and is the owner's decision with the piece that needs it. (3) *B1,* bands, for the frames that do not fit (built; its remaining costs are in its section below).

**Speed evidence.** A composed-world frame takes about 3 to 7 s. A blended full-canvas layer costs about 8 to 11 ns per pixel
per layer (normal blend 0.7 ns). A bloom on a magnified perspective layer took 71 s per render before a workaround (the glow
drawn in the drawing: 3 s). Sources: the 71 s in `bullterrier-demos/feedback/scene-m2.md`; the per-pixel figures were measured 2026-11-14 (C4a's own table in `api.md` has the blend-mode timings).

## Batches still to do, in order

1. ~~**Polish**~~ done below, bar the open rows.
2. ~~**C7** the per-layer cache, with the move-only fast path~~ done.
3. ~~**M3** the mill piece rebuilt as one scene~~ built (`docs/examples/mill.py`, demo 10 of `bullterrier-demos`).
4. ~~**O1** PNG and PDF output~~ done: `Raster::write`, `Raster::write_pdf`. Not built: a PDF that keeps vectors and live text (the PDF is raster), bleed from content beyond the canvas (it is the edge repeated), a black point or intent choice for CMYK.
5. ~~**P1** the Rust command line, `bt-scene render`~~ done: `docs/cli.md`.
6. **The final poster** (`bullterrier-demos`, demo 09, built on every library): the owner judges whether it is the final one.
7. After scene's first release: video (clock, mapping, encode), the web export, editor support (hit-test, tree edit), a
   frontend.

Room for light layers is built (C5c). B1 is built; banded looks wait for image's position-aware effects (B1 section below).

## B1: what banded rendering still lacks (2026-11-14)

Built: `render_bands`, `render_to_png`, `render_to_pdf`, a banded `bt-scene render` ("Banded rendering (B1)" in `design-decisions.md`).
What remains, with its evidence:

| What | Why it is not built | What would build it |
|---|---|---|
| **Banded looks.** A layer with a look is drawn whole once at its drawn size and cut per band (its raster stays resident: the memory floor of a banded render) | 40 of image's 62 effects read positions or the whole picture (measured by image, 2026-11-08), and image's `Place { canvas_height, canvas_width, first_row }` is internal: `Options` has no first-row setter. A look run on a band would differ from the whole | image: effects that take the band's first row and the canvas size; position-hashed noise for grain, dust and scratches. Then `Finish::run` runs a look per band with a margin of its footprint (`Recipe::spread` is already there) |
| **Drawings by window.** A drawing is drawn whole once (the 2^27 pixel limit stays on it) | vector's window render is within one level of its whole render at an edge pixel (vector's V10e note: "the cause was not isolated"); the cache-test poster's logo differed by one level at one pixel of a one-row band, so a banded render would not equal the whole | vector: a window that equals the whole bit for bit (the f32 coordinates of a shifted region); or the owner accepts one level for drawings, which makes the oracle a tolerance |
| **A PDF whose compressed image is not held.** `render_to_pdf` keeps the compressed colour (and alpha) streams until the file is assembled | pdf-writer writes whole objects | a hand-written writer of the one image object, or pdf-writer taking a stream; a vector PDF is a separate decision |
| **A plugin kind drawn by band** (asked for the band's rectangle) | a kind's pixels are promised by rectangle and scale, not by position: a kind with a seeded stream draws other pixels for another rectangle | a flag on the kind ("my pixels depend on the position alone") |
| **Reducing a picture shrunk past 2x once.** A picture shrunk by a map that averages it first (an A0 photo placed small) is averaged again for each band | the average is of the whole raster (`warp::reduce`); a time cost per band, not memory | the reduced raster kept in the cache by its key, or reduced for the needed rows only |
| **Scan-resistant cache for banded renders.** The cache's 2 GiB holds the looked layers and drawings of a banded render, and the planes of each band (not reused by another band) | a plane is kept by the target, which now includes the band's rows | planes not kept where only one band uses them |

### B1 completion: unblocked on 2026-11-15, not built (start here)

The two blockers in the first two rows above are lifted in the siblings; the scene side is the next batch:

1. **Bump the pins** (image and vector), as a commit of its own with the data-boundary tests green.
2. **Drawings by window.** Vector's window render now equals the crop of the whole render byte for byte (its V10e note says so; a window near its 65,000-pixel side limit, or one cutting a blurred shape too big to hold, is refused with a `ParameterError` naming `window`; one rare path, a gradient whose first two stops share offset 0, is not proven exact). In the drawing adapter, ask for the band's window instead of drawing whole; keep the C3 wrapper (frame exactly the raster, phase) so the window is a crop of that raster. The existing bit-exact band oracle (`render_bands` equals `render`, heights 1, 7, prime, half, full, non-dividing; sizes odd and even) is the proof; keep drawings whole where the window is refused.
3. **Banded looks.** Image now has `Options::with_window(first_row, canvas_width, canvas_height)` and `Recipe::band_margin(presets, longest_side) -> Option<usize>` (the margin rows a band needs, or `None` when the recipe cannot be banded: steps that read the whole picture, `each`/driven steps, steps with a `mask`/`partition`/field parameter, a `frame` or `size`). A looked layer whose recipes all have a margin can be drawn by band plus margin and looked per band; the others stay whole. Caveat from image: a blur summed by Fourier transform (reach of 16 working pixels or more) differs from the whole render by up to 1e-4, so those are banded within rounding, not bit for bit: either treat a recipe with such a blur as not bandable until image offers an exact-only `band_margin` (listed in image's roadmap), or state the tolerance. The bit-exact oracle decides.
4. **Room for every layer kind.** Image now has `Options::with_room(top, right, bottom, left)`: effects that place themselves by their picture (`each`, driven, whole-footprint steps, `Place` readers, whole-picture statistics) run on the original box, every other step on the grown picture. Limit: a boxed step does not reach a halo an earlier step painted into the room (a vignette after a bloom leaves the halo undarkened); order halo steps last, or give image's `Place` signed coordinates (its roadmap has the design). Scene's room (C5c) is only for light layers today because a black margin darkened opaque edges; with `with_room` scene can drop the padded-raster caveat and give room to every layer: decide per effect class with the oracle (grown render's box equals the box rendered alone).
5. **Plugin kinds by band:** add a flag on the kind ("my pixels depend on the position": default whole); a kind that passes the contract test "a drawn sub-rectangle equals the same crop of the whole rectangle" may be asked for the band's rectangle.
6. Docs, the limits table and the 15-layer A3 / A0 measurements are updated with each step; one full gate per batch.

## Polish found by building real pieces

Each item has the evidence that found it. Fix by the usual rule: reproduce at the public interface first.

| # | Item | Evidence |
|---|---|---|
| 1 | `import bullterrier_scene` from a source checkout fails with `PackageNotFoundError`: `__version__` reads package metadata at import | `src/bullterrier_scene/__init__.py` calls `metadata.version(...)`; a checkout without an installed distribution cannot import |
| 2 | A layer used only as the source of `Layer.of` references must be drawn. There is no "definition only" layer | M1 hid it with `opacity(0)` |
| 3 | A look on a layer used as a mask runs although only its alpha is read | reported by the lead agent; not reproduced yet (write the failing test first) |
| 4 | Rotating text moves it off the canvas: the pivot is the box centre. `api.md` needs a worked example (`anchor=(0, 1)`) | reported by the lead agent; write a render that shows it first |
| 5 | `api.md` says a look's `px` are "design pixels of the canvas" (Looks, step 3; "Design pixels"; "Design scale"). Since C5b they are image's design pixels of the canvas: 1/480 of the canvas long side. Fix the sentence everywhere, and the roadmap's C5 row | Conan poster: `outline(width=26)` made a ring 140 px wide on a 2700 px canvas; width 4.6 gives about 25 px (`bullterrier-demos/feedback/scene-conan.md`, 6b) |
| 6 | `Layer.text` takes a flat colour only | M2 used vector's `text` with a gradient fill directly |
| 7 | A picture cannot be trimmed to its matte | M2 cropped with numpy to put the reflection at the paws |
| 8 | No keyline helper | the Conan poster used image's `outline` on a second picture with `lighten`: works, awkward; a look cannot return its stroke as alpha only |
| 9 | A bloom on a magnified perspective layer takes 71 s (the perspective cap fix made it work). Performance of looks on large drawn rasters needs a look. **Open**; C5c adds room only to a light layer, at most up to image's 2^26 pixels, so a light bloom on such a layer is looked at a larger raster still | M2 (`scene-m2.md`, 1): C7 and B1 |
| 10 | ~~Room for looks (a halo past the layer box) and light spill~~ done (C5c): a light layer's looks run on its raster grown by image's `Recipe::spread`. **What remains:** a layer that is not light has none (its alpha is its source's); a recipe mixing a halo with a bounds effect shifts that effect by the room (two layers; image's `Options::with_room` would lift it); a look image cannot bound (`Unbounded`) gets none | `api.md`, "Looks" |
| 11 | `fit` then `place(dx, dy)` composes correctly in either order | verified on renders (`scene-conan.md`, 3): no gap, listed so nobody re-checks |

## Context for the next agent

- Read `CLAUDE.md`, `docs/api.md` (the code API and what is built) and `docs/design-decisions.md` (why).
- **What exists:** the whole core roadmap through C7, O1 and P1 (`roadmap.md`): layers of picture, drawing, text, fill, scene
  and plugin kinds (C8); `fit`, `place`, `perspective`; masks, maps, references, groups; looks on every layer; the sixteen
  blends and light; parameters and `with_values`; the saved scene; the cache; PNG and raster PDF output; `bt-scene`. **What is missing:**
  banded looks (B1 is built; looks wait for image), vector PDF,
  the open polish row 9, the web export, hit-test and tree edit for an editor.
- **The pieces** are in `bullterrier-demos` and use scene as a library from a checkout (`BT_SCENE_SRC`, the sibling's
  `src/`): 06 the two-ink poster, 07 the composed world, 08 the Conan poster, 09 the wow poster (a loop with a value set per
  frame: `with_values` then `render`), 10 the mill video. Its `feedback/scene-*.md` are the evidence of every polish row.
  Each demo builds one `Scene` from a function and renders it at several sizes; a frame loop reuses one scene, so the cache works.
- **The owner's rules:** speed over ceremony (`poe fast` per commit, one `poe check` per batch, a few hand mutants, no
  independent review unless asked); a gap found in image, vector or control while building a piece is fixed in that repository
  by its own rules without asking; pushes to the other repositories are authorised; no pushes of this one unless asked.
- The per-batch plans were written in the git-ignored `.superpowers/plans/`, which may be absent. The decisions are in
  `design-decisions.md`.
