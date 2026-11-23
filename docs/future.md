# After the core: what the directions led to

Set 2026-11-14, kept as the record of what was built on top of the core: the animated-poster direction, speed, banded rendering, print and the polish found by building real pieces. Nothing here redesigns scene: each item is built on what exists.

## The animated-poster direction, from scene

| Direction | What scene must provide | Status |
|---|---|---|
| Animated posters | `scene.with_values({...}).render(size)` per frame (built with C6a); the per-layer cache C7; a fast path for layers that only move; bands B1 for large frames. Video owns the clock, the signal mapping and the encode | cache and fast path built (C7); bands built (B1: looks are drawn whole, below) |

**The move-only fast path (built, C7).** `dx`, `dy`, `rotate`, `skew`, `flip`, `aspect`, `opacity` and `blend` only re-warp a kept raster of the layer; `scale` draws the source and runs its look again (a look's pixels follow the density). A drawing, a text and a group drawn at a fraction of a pixel also draw again when that fraction changes (the pixel phase is in their key); measured cheap in the mill (with the other values held, a logo moved by whole pixels takes 0.025 s a frame, by a fraction of a pixel 0.029 s), and the wow loop's dog is a picture, which has no phase.

**Measured with the cache** (release build, 2026-11-14; "Limits, measured" in `api.md` has the conditions): the wow poster's loop 0.506 s a frame before, 0.191 s after (re-measured 2026-11-16 with the demo's own builder: 0.95 s afresh, 0.19 s with the cache; "The bound is 2 GiB" in `design-decisions.md`); a mill frame with every value changing 0.524 s before, 0.413 s after (re-measured 2026-11-16 with the demo's own scene and photo, another machine: 0.30 s a frame, the same pixels at every commit, "The bound is 2 GiB" in `design-decisions.md`; and a mill-style frame 1.03 to 0.515 s once a look chain is kept stage by stage: the table in `api.md`); a mill frame that changes one value 0.025 to 0.24 s; a 15-layer A3 poster at 300 dpi 5.2 s the first time and 0.39 s with one headline changed, peak 3.15 GB against 2.16 GB.

**Speed steps taken:** (1) *A scan-resistant bound.* Built 2026-11-15 as a cache that spares what a banded render has used, and 2026-11-16 for a scene drawn again (a render that finds gone what the render before pushed out pushes out first what the render before used: "The bound is 2 GiB" in `design-decisions.md`). (2) *B1,* bands, for the frames that do not fit (built; its costs are in its section below).

**Speed evidence.** A composed-world frame takes about 3 to 7 s. A blended full-canvas layer costs about 8 to 11 ns per pixel
per layer (normal blend 0.7 ns). A bloom on a magnified perspective layer took 71 s per render before a workaround (the glow
drawn in the drawing: 3 s). Sources: the 71 s in `bullterrier-demos/feedback/scene-m2.md`; the per-pixel figures were measured 2026-11-14 (C4a's own table in `api.md` has the blend-mode timings).
WebAssembly and preview size (spike, 2026-11-16, nothing changed): the core, with image and vector at their pins, builds unchanged
for `wasm32-unknown-unknown` and `wasm32-wasip1`, and a saved scene renders under WASI (Node's V8, one thread), text layers
included. A 13-layer shirt print (a photo with a matte and image looks, vector rays and rings, text) at 1200 px long side,
median of 5 on a heavily loaded M4: cold 683 ms native, 988 ms WASM; after one value changes, 102 to 179 ms native and
182 to 287 ms WASM (grain on the top layer the cheapest, the photo's grade or dither the dearest); the same scene again,
everything cached, 76 ms native and 117 ms WASM. That last figure was every layer's placement and the composite. **Done 2026-11-17 (P2):** the composite below the changed layer is kept
(every other join of a whole render up to 2^22 pixels) and layers are laid on all cores. A similar 13-layer poster at 1,200 px long side, native, release, Apple M4, load average 3 to 7, median of 15: cold 164 ms before, 179 ms after; the same scene again 68 to 2 ms; a top layer's slider 70 to 4 ms; the bottom layer's slider 65 to 20 ms; an A3 poster in bands 2.4 to 2.6 s to 1.6 to 1.8 s; the pictures are the same within the one tolerance (`api.md`, "Limits, measured"). The WASM figures above were not measured again; `wasm32-wasip1` still builds. The scene's `.wasm` (scene, image and
vector) is 8.9 MB after `wasm-opt`, 2.5 MB brotli.

## Batches, in order

1. ~~**Polish**~~ done below.
2. ~~**C7** the per-layer cache, with the move-only fast path~~ done.
3. ~~**M3** the mill piece rebuilt as one scene~~ built (`docs/examples/mill.py`, demo 10 of `bullterrier-demos`).
4. ~~**O1** PNG and PDF output~~ done: `Raster::write`, `Raster::write_pdf`; the print batch (2026-11-17) added mirrored bleed, registration marks, DPI in PNG and JPEG, plates and an underbase. The PDF is raster (it keeps no vectors or live text), and bleed is the edge repeated or mirrored.
5. ~~**P1** the Rust command line, `bt-scene render`~~ done: `docs/cli.md`.
6. ~~Print output for dither, halftone and spot-ink pieces (plates, mirrored bleed, DPI, a white underbase)~~ done 2026-11-17: [dither-and-print.md](dither-and-print.md).

Room for light layers is built (C5c), and B1 with banded looks; what still renders whole is stated in "B1 completion" below.

## B1: what banded rendering still lacks (2026-11-14)

Built: `render_bands`, `render_to_png`, `render_to_pdf`, a banded `bt-scene render` ("Banded rendering (B1)" in `design-decisions.md`).
What remains, with its evidence:

| What | Why it is not built | What would build it |
|---|---|---|
| ~~**Banded looks.**~~ **Done 2026-11-15** (step 3 below): what stays whole is a looked layer whose recipe image cannot band, a layer not the canvas's size, a light layer; image answers only for recipes that band to the bit, so a Fourier-summed blur or `grain` past its switch stays whole (a banded render equals the whole one within the one tolerance) | | |
| ~~**Grain is banded within one float step.**~~ **Done 2026-11-15** (lesson L21): image's `band_margin` is exact-only (`4997d954`), scene's own 16-pixel guard is deleted, and the oracle runs a canvas of 3,900 pixels and a fractional shift far from the top. Was: **Grain is banded within one float step.** `grain` blurs its noise by a Fourier transform from a clump of about 8 working pixels (a picture of about 3,800 pixels on its long side at the default size: an A3 at 300 dpi), but its footprint is a pixel, so image reports margin 0 and the 16-pixel guard does not see it | measured 2026-11-15 on a 3,508 by 4,961 picture: banded render and whole render differ by at most 1.2e-7 (one float step) in 7 to 21 million values of the picture; a picture of 1,000 pixels is within the one tolerance (the oracle's sizes sum the noise tap by tap) | image's exact-only `Recipe::band_margin`, which must also answer for a step whose own blur is Fourier (Needs from others) | 
| ~~**A banded A3 of several looked layers past image's Fourier switch is slower than whole.**~~ **Done 2026-11-15:** a looked layer whose recipe image cannot band to the bit (`grain` past about 3,800 pixels on its long side, a Fourier blur) is drawn whole once and held, 278 MB each at A3; eight pass the cache's 2 GiB, and LRU drew each again for every band (40 draws, 33 evictions). The cache now spares what a banded render has used: seven stay, one is drawn per band (a render of one pass does not spare: a value it needs twice would be made twice); a scene drawn again keeps all but two layers (2026-11-16). | measured on an eight-layer stand-in, release, every core: banded 15.9 s to 4.5 s (whole 3.3 s), peak 3.3 to 3.4 GB (re-measured 2026-11-16 with the policy of one commit narrower: `design-decisions.md`, "The bound is 2 GiB"); the same slowdown existed at `b64a12c` (banded 32.7 s, whole 7.1 s) before any banded look | |
| ~~**A PDF whose compressed image is not held.**~~ **Done 2026-11-15:** the compressed colour and alpha go to scratch files beside the output and the PDF is assembled by copying them (`pdf.rs` writes the objects, table and trailer; pdf-writer writes the content stream). Held: a row and the codecs' buffers (0.8 MB for a 7.2 MB file; 29.7 MB before). Disk: the two files, as large as the compressed image | | |
| ~~**A plugin kind drawn by band**~~ **Done 2026-11-15** (step 5): a kind says `by_position` | | |
| ~~**Reducing a picture shrunk past 2x once.**~~ **Done 2026-11-15:** a band lands from the average, made once and kept in the cache (`reduced`); only a map whose footprint is a rectangle along the axes averages first | | |
| ~~**Scan-resistant cache for banded renders.**~~ **Done 2026-11-15:** the plane of a band is kept as expendable (`Cache::fetch_expendable`): only where it costs no finished layer, and the first to go. What else a band keeps that no other band reads is not measured | | |

### B1 completion: unblocked on 2026-11-15; steps 1 to 5 done the same day, step 6 measured (`design-decisions.md`)

The two blockers in the first two rows above are lifted in the siblings; the scene side is the next batch:

1. **Bump the pins (done)** (image and vector), as a commit of its own with the data-boundary tests green.
2. **Drawings by window. Done 2026-11-15.** Vector's window render now equals the crop of the whole render within the one tolerance (its V10e note says so; a window near its 65,000-pixel side limit, or one cutting a blurred shape too big to hold, is refused with a `ParameterError` naming `window`; one rare path, a gradient whose first two stops share offset 0, is not proven exact). In the drawing adapter, ask for the band's window instead of drawing whole; keep the C3 wrapper (frame exactly the raster, phase) so the window is a crop of that raster. The existing bit-exact band oracle (`render_bands` equals `render`, heights 1, 7, prime, half, full, non-dividing; sizes odd and even) is the proof; a window vector refuses (a side past 65,000 pixels, 2^27 in all) is a raster its whole render refuses too, so there is no whole fallback.
3. **Banded looks. Done 2026-11-15** (a looked picture, drawing or group is drawn for the rows of a band and the margin its looks reach; recipes image cannot band to the bit (a Fourier-summed blur, `grain` past its switch), a layer not the canvas's size and a light layer stay whole: "Banded looks" in `design-decisions.md`). Image now has `Options::with_window(first_row, canvas_width, canvas_height)` and `Recipe::band_margin(presets, longest_side) -> Option<usize>` (the margin rows a band needs, or `None` when the recipe cannot be banded: steps that read the whole picture, `each`/driven steps, steps with a `mask`/`partition`/field parameter, a `frame` or `size`). A looked layer whose recipes all have a margin can be drawn by band plus margin and looked per band; the others stay whole. A blur summed by Fourier transform differs from the whole render in the last bits, so image's `band_margin` (exact-only since `4997d954`) answers none for it and scene runs it whole; the bit-exact oracle decides.
4. **Room for every layer kind. Done 2026-11-15** for what room can show: a light layer's looks now run with `Options::with_room`, so a step that places itself by its picture is placed by the box (the oracle: image's render of the box with room). A layer that is not light still has none: its alpha is its source's, so there is no alpha for a halo to show in; the black margin that darkened its blurred edges is not the question while it has no room.
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
| 8 | ~~No keyline helper~~ **Done** (9132201 (`api.md` recipe: a keyline)) | the Conan poster used image's `outline` on a second picture with `lighten`: works, awkward; a look cannot return its stroke as alpha only |
| 9 | A bloom on a magnified perspective layer takes 71 s (the perspective cap fix made it work). **Done 2026-11-15 by threads.** **Measured 2026-11-16, one probe** (release build; a 600 by 600 drawing of 24 by 24 cells on the quadrilateral (500, 50), (700, 50), (2600, 1400), (-1400, 1400) of a 1,200 by 800 canvas, a light bloom `radius` 40 `threshold` 0.7 on it, rendered at 1,200; three runs interleaved, medians, one-minute load 3.1 to 4.6 at the start of each run; the same pixels at every pin and thread count, one hash each): no look 1.97 s on one thread at every pin; the bloom on one thread 26.6 s with image `11eedda9`, 25.7 s with `72865bfd` (the faster transpose), 25.8 s with `7b07ff72` (now), and on every core 9.1, 9.4 and 9.3 s. **The lever is threads (2.8 times); the image pins are within 3%, the spread between runs** (image's own benchmark of its bloom on a square picture gave 1.4 times for the transpose; it does not show on this layer). Scene sets `Options::with_threads(0)` where it runs a look (`Look::apply`): image keeps one pool of that size for the process, so renders running at once (Python releases the lock) share it and do not start a pool each; image's `with_threads` documentation and tests hold the output the same at any count. The figures of the earlier probes (3.7 and 21.2 s; 2.3 and 14.7 s; a 1,200 by 800 grid of 24 cells 3.9 to 1.5 s) were other pictures and are replaced by this table. A 15-layer A3 stand-in with looked drawings and pictures, a bloom and large pictures: whole 15.4 to 6.4 s, banded in 1,000 rows 70.5 to 27.4 s (one run each), with the same peak memory within the run-to-run spread. | M2 (`scene-m2.md`, 1): C7 and B1 |
| 10 | ~~Room for looks (a halo past the layer box) and light spill~~ done (C5c; a bounds effect is placed by the box since `with_room`, 2026-11-15): a light layer's looks run on its raster grown by image's `Recipe::spread`.  A layer that is not light has none (its alpha is its source's: no alpha for a halo to show in) | `api.md`, "Looks" |
| 11 | `fit` then `place(dx, dy)` composes correctly in either order | verified on renders (`scene-conan.md`, 3): no gap, listed so nobody re-checks |
| 12 | ~~A look's `px` cannot be given in output pixels or as a share of the canvas: a look carried over from a picture of another size is rescaled by hand (`480 / long side`), and inside a group by the group's size again~~ **Done** (270cb54, 0bd8e7c: a look's `px="canvas"`, `Px::Canvas`; `design-decisions.md`, "A look's `px` in canvas pixels") | the mill (`PX = 480 / max(W, H)`), the wow poster's story group (looks scaled by `H / h`) |
| 13 | ~~ Worked answers missing in `api.md`: a halo only (the glow masked by its inverted source), one look on the whole scene (a group round everything), `aspect` squashing toward the anchor (the shadow's `dy`), stroke widths under `perspective` (drawn flat, so near lines grow)~~ **Done** (91fe1fb, 28436eb, 80bbc90, e57f877) | M2, the mill |
| 14 | ~~Stale text: rows 2 to 8 are answered (`api.md` recipes, commits d1597b2, 58386bf, 9132201) but not marked done; `design-decisions.md` still calls a look's `px` a canvas pixel in two places; `roadmap.md` says vector's V10e is not built~~ **Done** (this batch) | checked 2026-11-15 |
| 17 | The next image pin brings the dither methods and the halftone batch (2026-11-16). **Pinned 2026-11-17** (image `59ed56e9`, with its vectorised pixel loops and sampler fast path): the one test that read halftone's `cell` as a whole number now reads bloom's `octaves`; demos 06, 09 and 10 re-rendered the same day (09 is byte-identical, it has no halftone; 06's dots and 10's line screen print a little lighter, as tone-true halftones should): every halftone picture changes tone (the round dot printed about pi/2 x the tone; separations solved by density printed too light: both now tone-true), dither's `separation="inks"` is now `"spot"`, and `cell` is fractional, so demo 10's night halftone can move to `px="canvas"`. `tests/it/data_boundary.rs` will name the changes; re-render demos 06, 09 and 10 after the bump | image `docs/design-decisions.md` (this batch) |

## Context for an agent starting here

- Read `CLAUDE.md`, `docs/api.md` (the code API and what is built) and `docs/design-decisions.md` (why).
- **What exists:** the whole core roadmap through C7, O1 and P1 (`roadmap.md`): layers of picture, drawing, text, fill, scene
  and plugin kinds (C8); `fit`, `place`, `perspective`; masks, maps, references, groups; looks on every layer; the sixteen
  blends and light; parameters and `with_values`; the saved scene; the cache; PNG and raster PDF output; `bt-scene`.
- **The pieces** are in `bullterrier-demos` and use scene as a library from a checkout (`BT_SCENE_SRC`, the sibling's
  `src/`): 06 the two-ink poster, 07 the composed world, 08 the Conan poster, 09 the wow poster (a loop with a value set per
  frame: `with_values` then `render`), 10 the mill video. Its `feedback/scene-*.md` are the evidence of every polish row.
  Each demo builds one `Scene` from a function and renders it at several sizes; a frame loop reuses one scene, so the cache works.
- **The owner's rules:** speed over ceremony (`poe fast` per commit, one `poe check` per batch, a few hand mutants, no
  independent review unless asked); a gap found in image, vector or control while building a piece is fixed in that repository
  by its own rules without asking; pushes to the other repositories are authorised; no pushes of this one unless asked.
- The per-batch plans were written in the git-ignored `.superpowers/plans/`, which may be absent. The decisions are in
  `design-decisions.md`.

## The GPU compositor: what is left (BTSCN-0010, 2026-11-22)

Built: warps, area average, footprint, masks, feather, the sixteen blends, light, a picture's looks through image's executor, the
device cache, `Gpu`, Python `Gpu` and `render(gpu=)`, `bt-scene render --gpu`. Speed evidence: `api.md`, "Limits, measured" (a layer low
in the stack changed: 4.5 to 7 times the CPU's time at 1,920; a repeat the CPU's kept joins answer in 3 to 9 ms is 19 to 35 ms on the
device). Not built, in the order they pay:

1. **Joins kept on the device** (the CPU keeps the join after every other layer): a repeat and a top-layer slider would be one blend.
2. **Per-dispatch cost:** a uniform buffer and a bind group are made for each of the roughly three kernels a layer takes; a pool of
   them, or one bind group of the layer's parameters, would cut the recording.
3. **Groups, and layers the CPU draws, on the device:** a group recurses on the CPU and is sent; text and drawings wait for vector's
   GPU path (`BTVEC-0014`).
4. **Bands on the device** (`render_to_png` with a GPU, a picture past what the device binds): bands sized from the device's limit
   through `render_bands`.
5. **A light layer's looks and a look that uses a map** on the device (room, `render_from` with maps).
6. **Image's resample kernel** (`BTIMG-0012`): upload the decoded file once and `render_from`, instead of the CPU framing and resizing
   a picture before it is sent.
7. **The recording asynchronous**, for the browser (`BTSCN-0011`): image's calls are polled once now and are ready on native.
8. **The footprint of a perspective in 32-bit floats** differs from the CPU's where a footprint's length is just over a whole number
   (a tap more or fewer): the bound is 1e-2; a table of the singular values worked out on the CPU would make it exact.
