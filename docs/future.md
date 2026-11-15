# Future: what scene must provide after the first release

Set 2026-11-14. The ecosystem view (three directions, order, library changes in every repo) is in the design repository's
`future.md`. This file is scene's part. Nothing here redesigns scene: each item is built on what exists.

## The three directions, from scene

| Direction | What scene must provide | Status |
|---|---|---|
| Animated posters | `scene.with_values({...}).render(size)` per frame (built with C6a); the per-layer cache C7; a fast path for layers that only move; bands B1 for large frames. Video owns the clock, the signal mapping and the encode | cache and fast path built (C7); bands built (B1: looks are drawn whole, below) |
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
8. Print output for dither, halftone and spot-ink pieces (plates, mirrored bleed, DPI, a white underbase), once image
   hands out its ink planes: [dither-and-print.md](dither-and-print.md).

Room for light layers is built (C5c), and B1 with banded looks; what still renders whole is stated in "B1 completion" below.

## B1: what banded rendering still lacks (2026-11-14)

Built: `render_bands`, `render_to_png`, `render_to_pdf`, a banded `bt-scene render` ("Banded rendering (B1)" in `design-decisions.md`).
What remains, with its evidence:

| What | Why it is not built | What would build it |
|---|---|---|
| ~~**Banded looks.**~~ **Done 2026-11-15** (step 3 below): what stays whole is a looked layer whose recipe image cannot band, a layer not the canvas's size, a light layer; image answers only for recipes that band to the bit, so a Fourier-summed blur or `grain` past its switch stays whole (a banded render equals the whole one bit for bit) | | |
| ~~**Grain is banded within one float step.**~~ **Done 2026-11-15** (lesson L21): image's `band_margin` is exact-only (`4997d954`), scene's own 16-pixel guard is deleted, and the oracle runs a canvas of 3,900 pixels and a fractional shift far from the top. Was: **Grain is banded within one float step.** `grain` blurs its noise by a Fourier transform from a clump of about 8 working pixels (a picture of about 3,800 pixels on its long side at the default size: an A3 at 300 dpi), but its footprint is a pixel, so image reports margin 0 and the 16-pixel guard does not see it | measured 2026-11-15 on a 3,508 by 4,961 picture: banded render and whole render differ by at most 1.2e-7 (one float step) in 7 to 21 million values of the picture; a picture of 1,000 pixels is bit for bit (the oracle's sizes sum the noise tap by tap) | image's exact-only `Recipe::band_margin`, which must also answer for a step whose own blur is Fourier (Needs from others) | 
| **A banded A3 of several looked layers past image's Fourier switch is slower than whole.** A looked layer whose recipe image cannot band to the bit (`grain` past about 3,800 pixels on its long side, a Fourier blur) is drawn whole once and held: 278 MB each at A3, so eight of them pass the cache's 2 GiB and are drawn again for the bands | measured 2026-11-15 (release, every core): a 15-layer stand-in takes 27 s banded and 6.4 s whole with eight such layers, and 4.3 s banded and 4.2 s whole with a grade in their place; the cause is probable, the cache not measured | image: `grain` summed tap by tap or banded exactly past its switch; or a cache that holds finished looked layers outside the 2 GiB bound a banded render |
| ~~**A PDF whose compressed image is not held.**~~ **Done 2026-11-15:** the compressed colour and alpha go to scratch files beside the output and the PDF is assembled by copying them (`pdf.rs` writes the objects, table and trailer; pdf-writer writes the content stream). Held: a row and the codecs' buffers (0.8 MB for a 7.2 MB file; 29.7 MB before). Disk: the two files, as large as the compressed image | | |
| ~~**A plugin kind drawn by band**~~ **Done 2026-11-15** (step 5): a kind says `by_position` | | |
| ~~**Reducing a picture shrunk past 2x once.**~~ **Done 2026-11-15:** a band lands from the average, made once and kept in the cache (`reduced_for_bands`); only a map whose footprint is a rectangle along the axes averages first | | |
| ~~**Scan-resistant cache for banded renders.**~~ **Done 2026-11-15:** the plane of a band is kept as expendable (`Cache::fetch_expendable`): only where it costs no finished layer, and the first to go. What else a band keeps that no other band reads is not measured | | |

### B1 completion: unblocked on 2026-11-15; steps 1 to 5 done the same day, step 6 measured (`design-decisions.md`)

The two blockers in the first two rows above are lifted in the siblings; the scene side is the next batch:

1. **Bump the pins (done)** (image and vector), as a commit of its own with the data-boundary tests green.
2. **Drawings by window. Done 2026-11-15.** Vector's window render now equals the crop of the whole render byte for byte (its V10e note says so; a window near its 65,000-pixel side limit, or one cutting a blurred shape too big to hold, is refused with a `ParameterError` naming `window`; one rare path, a gradient whose first two stops share offset 0, is not proven exact). In the drawing adapter, ask for the band's window instead of drawing whole; keep the C3 wrapper (frame exactly the raster, phase) so the window is a crop of that raster. The existing bit-exact band oracle (`render_bands` equals `render`, heights 1, 7, prime, half, full, non-dividing; sizes odd and even) is the proof; a window vector refuses (a side past 65,000 pixels, 2^27 in all) is a raster its whole render refuses too, so there is no whole fallback.
3. **Banded looks. Done 2026-11-15** (a looked picture, drawing or group is drawn for the rows of a band and the margin its looks reach; recipes image cannot band to the bit (a Fourier-summed blur, `grain` past its switch), a layer not the canvas's size and a light layer stay whole: "Banded looks" in `design-decisions.md`). Image now has `Options::with_window(first_row, canvas_width, canvas_height)` and `Recipe::band_margin(presets, longest_side) -> Option<usize>` (the margin rows a band needs, or `None` when the recipe cannot be banded: steps that read the whole picture, `each`/driven steps, steps with a `mask`/`partition`/field parameter, a `frame` or `size`). A looked layer whose recipes all have a margin can be drawn by band plus margin and looked per band; the others stay whole. A blur summed by Fourier transform differs from the whole render in the last bits, so image's `band_margin` (exact-only since `4997d954`) answers none for it and scene runs it whole; the bit-exact oracle decides.
4. **Room for every layer kind. Done 2026-11-15** for what room can show: a light layer's looks now run with `Options::with_room`, so a step that places itself by its picture is placed by the box (the oracle: image's render of the box with room). A layer that is not light still has none: its alpha is its source's, so there is no alpha for a halo to show in; the black margin that darkened its blurred edges is not the question while it has no room. Open: a boxed step after a halo step does not reach the halo (image's signed `Place`).
5. **Plugin kinds by band. Done 2026-11-15:** `LayerKind::by_position` (default false; Python `by_position=True`); a kind that says yes is asked for the rows a band reads (and the margin of its looks), and `check_by_position` (public) holds it to "a drawn sub-rectangle equals the same crop of the whole rectangle".
6. **Docs, the limits table and the 15-layer A3 / A0 measurements. Done 2026-11-15** (`design-decisions.md`, "Measured 2026-11-15"; banded A3 peak 3.25 to 2.96 GB on the new stand-in).

## Polish found by building real pieces

Each item has the evidence that found it. Fix by the usual rule: reproduce at the public interface first.

| # | Item | Evidence |
|---|---|---|
| 1 | ~~`import bullterrier_scene` from a source checkout fails with `PackageNotFoundError`: `__version__` reads package metadata at import~~ **Done** (88a3446) | `src/bullterrier_scene/__init__.py` calls `metadata.version(...)`; a checkout without an installed distribution cannot import |
| 2 | ~~A layer used only as the source of `Layer.of` references must be drawn. There is no "definition only" layer~~ **Done** (d1597b2) | M1 hid it with `opacity(0)` |
| 3 | ~~A look on a layer used as a mask runs although only its alpha is read~~ **Done** (58386bf) | reported by the lead agent; not reproduced yet (write the failing test first) |
| 4 | ~~Rotating text moves it off the canvas: the pivot is the box centre. `api.md` needs a worked example (`anchor=(0, 1)`)~~ **Done** (9132201) | reported by the lead agent; write a render that shows it first |
| 5 | ~~`api.md` says a look's `px` are "design pixels of the canvas" (Looks, step 3; "Design pixels"; "Design scale"). Since C5b they are image's design pixels of the canvas: 1/480 of the canvas long side. Fix the sentence everywhere, and the roadmap's C5 row~~ **Done** (a27fd03, 9132201) | Conan poster: `outline(width=26)` made a ring 140 px wide on a 2700 px canvas; width 4.6 gives about 25 px (`bullterrier-demos/feedback/scene-conan.md`, 6b) |
| 6 | ~~`Layer.text` takes a flat colour only~~ **Done** (9132201 (`api.md` recipe: a gradient on type)) | M2 used vector's `text` with a gradient fill directly |
| 7 | ~~A picture cannot be trimmed to its matte~~ **Done** (9132201 (`api.md` recipe: trim to the matte)) | M2 cropped with numpy to put the reflection at the paws |
| 8 | ~~No keyline helper~~ **Done** (9132201 (`api.md` recipe: a keyline; image returning the stroke as alpha stays listed under "Needs from others")) | the Conan poster used image's `outline` on a second picture with `lighten`: works, awkward; a look cannot return its stroke as alpha only |
| 9 | A bloom on a magnified perspective layer takes 71 s (the perspective cap fix made it work). **Done 2026-11-15 by threads.** **Measured** (release build, one run each, load unknown): a 600 by 600 grid drawing on a quadrilateral whose near side is magnified, on a 1,200 by 800 canvas, on one thread: 2.3 s with no look, 14.7 s with a light bloom, 14.2 s with image's faster transpose (`72865bfd`: about 4% here, 1.4 times in image's own bloom). A lighter grid on the same canvas (24 cells) takes 0.15 s with no look and, with a light bloom, 3.9 s on one thread and 1.5 s on every core, the same pixels: the lever is threads. A 15-layer A3 stand-in with looked drawings and pictures, a bloom and large pictures: whole 15.4 to 6.4 s, banded in 1,000 rows 70.5 to 27.4 s, with the same peak memory within the run-to-run spread (4.9 to 5.4 GB whole, 5.2 to 5.5 GB banded, on or off). Scene sets `Options::with_threads(0)` where it runs a look (`Look::apply`): image keeps one pool of that size for the process, so renders running at once (Python releases the lock) share it and do not start a pool each; image's `with_threads` documentation and tests hold the output the same at any count. **What stays:** the look runs on the layer at its largest scale, up to image's 2^26 pixels, and neither banded looks nor room change that (a perspective layer is not banded) | M2 (`scene-m2.md`, 1): C7 and B1 |
| 10 | ~~Room for looks (a halo past the layer box) and light spill~~ done (C5c; a bounds effect is placed by the box since `with_room`, 2026-11-15): a light layer's looks run on its raster grown by image's `Recipe::spread`. **What remains:** a layer that is not light has none (its alpha is its source's: no alpha for a halo to show in); a boxed step after a halo step does not reach the halo (order halo steps last, or image's signed `Place`); a look image cannot bound (`Unbounded`) gets none | `api.md`, "Looks" |
| 11 | `fit` then `place(dx, dy)` composes correctly in either order | verified on renders (`scene-conan.md`, 3): no gap, listed so nobody re-checks |
| 12 | A look's `px` cannot be given in output pixels or as a share of the canvas: a look carried over from a picture of another size is rescaled by hand (`480 / long side`), and inside a group by the group's size again | the mill (`PX = 480 / max(W, H)`), the wow poster's story group (looks scaled by `H / h`) |
| 13 | Worked answers missing in `api.md`: a halo only (the glow masked by its inverted source), one look on the whole scene (a group round everything), `aspect` squashing toward the anchor (the shadow's `dy`), stroke widths under `perspective` (drawn flat, so near lines grow) | M2, the mill |
| 14 | ~~Stale text: rows 2 to 8 are answered (`api.md` recipes, commits d1597b2, 58386bf, 9132201) but not marked done; `design-decisions.md` still calls a look's `px` a canvas pixel in two places; `roadmap.md` says vector's V10e is not built~~ **Done** (this batch) | checked 2026-11-15 |

## Context for the next agent

- Read `CLAUDE.md`, `docs/api.md` (the code API and what is built) and `docs/design-decisions.md` (why).
- **What exists:** the whole core roadmap through C7, O1 and P1 (`roadmap.md`): layers of picture, drawing, text, fill, scene
  and plugin kinds (C8); `fit`, `place`, `perspective`; masks, maps, references, groups; looks on every layer; the sixteen
  blends and light; parameters and `with_values`; the saved scene; the cache; PNG and raster PDF output; `bt-scene`. **What is missing:**
  vector PDF, banded looks of a recipe image cannot band,
  the open polish row 9 (the cost of a bloom on a very large layer), the web export, hit-test and tree edit for an editor.
- **The pieces** are in `bullterrier-demos` and use scene as a library from a checkout (`BT_SCENE_SRC`, the sibling's
  `src/`): 06 the two-ink poster, 07 the composed world, 08 the Conan poster, 09 the wow poster (a loop with a value set per
  frame: `with_values` then `render`), 10 the mill video. Its `feedback/scene-*.md` are the evidence of every polish row.
  Each demo builds one `Scene` from a function and renders it at several sizes; a frame loop reuses one scene, so the cache works.
- **The owner's rules:** speed over ceremony (`poe fast` per commit, one `poe check` per batch, a few hand mutants, no
  independent review unless asked); a gap found in image, vector or control while building a piece is fixed in that repository
  by its own rules without asking; pushes to the other repositories are authorised; no pushes of this one unless asked.
- The per-batch plans were written in the git-ignored `.superpowers/plans/`, which may be absent. The decisions are in
  `design-decisions.md`.
