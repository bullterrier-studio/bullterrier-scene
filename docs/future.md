# Future: what scene must provide after the first release

Set 2026-11-14. The ecosystem view (three directions, order, library changes in every repo) is in the design repository's
`future.md`. This file is scene's part. Nothing here redesigns scene: each item is built on what exists.

## The three directions, from scene

| Direction | What scene must provide | Status |
|---|---|---|
| Animated posters | `scene.with_values({...}).render(size)` per frame (built with C6a); the per-layer cache C7; a fast path for layers that only move; bands B1 for large frames. Video owns the clock, the signal mapping and the encode | cache and fast path built (C7); bands not built |
| Websites | A web export target that reads the scene JSON (C6b, built): each layer an image or a vector, placed with CSS transforms, masks as CSS masks, blends as CSS `mix-blend-mode` (the W3C set by CSS names, on purpose), links as hit areas. Effects are baked into the layer images. Also a render worker (design README, "As a paid web app") | JSON built (C6b); export not planned |
| Drag-and-drop editor | `scene.parameters()` for controls (unit, range, meaning per address); a hit-test call ("which layer is under this point"); an edit API for the layer tree (add, remove, reorder, regroup); a draft at a smaller size (built: `render(size)`); undo as immutable values (built) | parameters in progress; hit-test and tree edit not planned |

**The move-only fast path (built, C7).** `dx`, `dy`, `rotate`, `skew`, `flip`, `aspect`, `opacity` and `blend` only re-warp a kept raster of the layer; `scale` draws the source and runs its look again (a look's pixels follow the density). A drawing, a text and a group drawn at a fraction of a pixel also draw again when that fraction changes (the pixel phase is in their key); measured cheap in the mill (with the other values held, a logo moved by whole pixels takes 0.025 s a frame, by a fraction of a pixel 0.029 s), and the wow loop's dog is a picture, which has no phase.

**Measured with the cache** (release build, 2026-11-14; "Limits, measured" in `api.md` has the conditions): the wow poster's loop 0.506 s a frame before, 0.191 s after; a mill frame with every value changing 0.524 s before, 0.413 s after; a mill frame that changes one value 0.025 to 0.24 s; a 15-layer A3 poster at 300 dpi 5.2 s the first time and 0.39 s with one headline changed, peak 3.15 GB against 2.16 GB.

**The next speed steps, by what they would save:** (1) *a look chain kept stage by stage.* A mill frame changes the swell of two references, each running the night grade first and then its swell: 0.41 s, of which the grade is about half. Keeping the raster after each look (with the maps it needs) takes the frame to the swells alone; changing a look in the middle of a chain runs only the looks after it. (2) *A scan-resistant bound.* A render that scans more than the 2 GiB keeps and reuses nothing; keeping the layers that were the costliest to draw, or the most recent ones of a scan, would make a poster slightly over the bound still fast. (3) *A look that follows the phase less.* A looked drawing that shakes by a fraction of a pixel runs its look again each frame; a look drawn at phase 0 and shifted by the warp would not, and softens by one resample, so it changes renders and is the owner's decision with the piece that needs it. (4) *B1,* bands, for the frames that do not fit.

**Speed evidence.** A composed-world frame takes about 3 to 7 s. A blended full-canvas layer costs about 8 to 11 ns per pixel
per layer (normal blend 0.7 ns). A bloom on a magnified perspective layer took 71 s per render before a workaround (the glow
drawn in the drawing: 3 s). Sources: the 71 s in `bullterrier-demos/feedback/scene-m2.md`; the per-pixel figures were measured 2026-11-14 (C4a's own table in `api.md` has the blend-mode timings).

## Batches still to do, in order

1. ~~**Polish**~~ done below, bar the open rows.
2. ~~**C7** the per-layer cache, with the move-only fast path~~ done.
3. ~~**M3** the mill piece rebuilt as one scene~~ built (`docs/examples/mill.py`, demo 10 of `bullterrier-demos`).
4. ~~**O1** PNG output~~ done: `Raster::write`.
5. ~~**P1** the Rust command line, `bt-scene render`~~ done: `docs/cli.md`.
6. **The final poster** (`bullterrier-demos`, demo 09, built on every library): the owner judges whether it is the final one.
7. After scene's first release: video (clock, mapping, encode), the web export, editor support (hit-test, tree edit), a
   frontend.

C5b room (a halo or light spill past a layer's box) waits for image's sideways footprint ("Needs from others" in
`api.md`). B1 stays deferred until image's position-aware effects and canvas hand-over exist.

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
| 9 | A bloom on a magnified perspective layer takes 71 s (the perspective cap fix made it work). Performance of looks on large drawn rasters needs a look | M2 (`scene-m2.md`, 1): C7 and B1 |
| 10 | Room for looks (a halo past the layer box) and light spill | needs image's sideways footprint (open row in "Needs from others") |
| 11 | `fit` then `place(dx, dy)` composes correctly in either order | verified on renders (`scene-conan.md`, 3): no gap, listed so nobody re-checks |

## Context for the next agent

- Read `CLAUDE.md`, `docs/api.md` (the code API and what is built) and `docs/design-decisions.md` (why).
- **What exists:** the whole core roadmap through C7, O1 and P1 (`roadmap.md`): layers of picture, drawing, text, fill, scene
  and (Rust) plugin kinds; `fit`, `place`, `perspective`; masks, maps, references, groups; looks on every layer; the sixteen
  blends and light; parameters and `with_values`; the saved scene; the cache; PNG output; `bt-scene`. **What is missing:**
  room for looks (C5b, image's footprint), bands (B1, deferred), PDF and print colour, Python and JSON for plugin kinds,
  the open polish rows 9 and 10, the look chain cache above, the web export, hit-test and tree edit for an editor.
- **The pieces** are in `bullterrier-demos` and use scene as a library from a checkout (`BT_SCENE_SRC`, the sibling's
  `src/`): 06 the two-ink poster, 07 the composed world, 08 the Conan poster, 09 the wow poster (a loop with a value set per
  frame: `with_values` then `render`), 10 the mill video. Its `feedback/scene-*.md` are the evidence of every polish row.
  Each demo builds one `Scene` from a function and renders it at several sizes; a frame loop reuses one scene, so the cache works.
- **The owner's rules:** speed over ceremony (`poe fast` per commit, one `poe check` per batch, a few hand mutants, no
  independent review unless asked); a gap found in image, vector or control while building a piece is fixed in that repository
  by its own rules without asking; pushes to the other repositories are authorised; no pushes of this one unless asked.
- The per-batch plans were written in the git-ignored `.superpowers/plans/`, which may be absent. The decisions are in
  `design-decisions.md`.
