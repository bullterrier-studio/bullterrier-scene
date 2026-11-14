# Future: what scene must provide after the first release

Set 2026-11-14. The ecosystem view (three directions, order, library changes in every repo) is in the design repository's
`future.md`. This file is scene's part. Nothing here redesigns scene: each item is built on what exists.

## The three directions, from scene

| Direction | What scene must provide | Status |
|---|---|---|
| Animated posters | `scene.with_values({...}).render(size)` per frame (built with C6a); the per-layer cache C7; a fast path for layers that only move; bands B1 for large frames. Video owns the clock, the signal mapping and the encode | cache, fast path and bands not built |
| Websites | A web export target that reads the scene JSON (C6b, built): each layer an image or a vector, placed with CSS transforms, masks as CSS masks, blends as CSS `mix-blend-mode` (the W3C set by CSS names, on purpose), links as hit areas. Effects are baked into the layer images. Also a render worker (design README, "As a paid web app") | JSON built (C6b); export not planned |
| Drag-and-drop editor | `scene.parameters()` for controls (unit, range, meaning per address); a hit-test call ("which layer is under this point"); an edit API for the layer tree (add, remove, reorder, regroup); a draft at a smaller size (built: `render(size)`); undo as immutable values (built) | parameters in progress; hit-test and tree edit not planned |

**The move-only fast path.** `dx`, `dy`, `rotate`, `skew` and `flip` only re-warp a cached raster of the layer. `scale` redraws
the source and its look (a look's pixels follow the density). Part of C7: the raster's cache key leaves out the move fields.

**Speed evidence.** A composed-world frame takes about 3 to 7 s. A blended full-canvas layer costs about 8 to 11 ns per pixel
per layer (normal blend 0.7 ns). A bloom on a magnified perspective layer took 71 s per render before a workaround (the glow
drawn in the drawing: 3 s). Sources: the 71 s in `bullterrier-demos/feedback/scene-m2.md`; the per-pixel figures were measured 2026-11-14 (C4a's own table in `api.md` has the blend-mode timings).

## Batches still to do, in order

1. **Polish** the list below.
2. **C7** the per-layer cache, with the move-only fast path.
3. **M3** the mill piece rebuilt as one scene.
4. **O1** PNG output.
5. **P1** the Rust command line, `bt-scene render`.
6. **The final poster** in `bullterrier-demos`, using every library (the owner wants it to be properly impressive).
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
- The per-batch plans were written in the git-ignored `.superpowers/plans/`, which may be absent. The decisions are in
  `design-decisions.md`.
- Process is lean: `poe fast` per commit, one full gate (`poe check`) per batch. A gap found in image, vector or control
  while building a piece is fixed in that repository without asking; pushes to the other repositories are authorised.
- The real pieces are in `bullterrier-demos`: 06 the two-ink poster, 07 the composed world, 08 the Conan poster, 04 the mill
  video; `feedback/scene-*.md` hold the gap notes.
