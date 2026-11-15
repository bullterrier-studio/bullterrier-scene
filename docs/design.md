# Design

What bullterrier-scene is and how it is built. Reasons for single choices: [design-decisions.md](design-decisions.md).
The batches, in order: [roadmap.md](roadmap.md). The calls: [api.md](api.md).
Status: draft for review, 2026-11-08; module shape and open questions updated 2026-11-13 (A1).

## What it is

A scene is a canvas and a stack of layers, drawn into one picture at any size. A layer is a source (a picture, a vector
drawing, type, a flat colour, another scene, another layer of the same scene, a picture the caller hands in each time) plus
how it sits on the layers below it: its place, its mask, its blend mode, its opacity and the effects it gets. Layers can be
grouped (a scene used as a layer), and a group blends as one.

| Responsible for | Not responsible for |
|---|---|
| The canvas and the stack of layers, in groups | Effects on one picture (grade, halftone, glitch) → image |
| Placing a layer: position, rotation, scale, skew, perspective, anchor, fit | Shapes, type, booleans, SVG → vector |
| Masks: from a shape or text, an alpha, luminance, a given map, a subject cut-out; soft or hard; inverted | Finding a subject or a depth map → image |
| Blend modes (the W3C set), opacity, groups that blend as one | Values changing over time → control |
| An effect chain on a layer or a group, applied at the right size | Footage, a timeline, sound, encoding → video |
| Describing every parameter of the scene (name, unit, range, meaning) | An editor or painting tools |
| Rendering in bands; caching per layer | built (B1, C7) |
| Output: a picture, PNG, a raster PDF with bleed, crop marks and CMYK through a caller's profile | |

## What it is for

A poster is the first piece because it is the strictest test of the layer model (cut-outs, type between layers, effects, blends),
not the only goal. Every use below is the same scene with different layers, values or output.

| Use | What it asks of scene | Batches |
|---|---|---|
| Poster, cover, social card, thumbnail | layers, placement, masks, blends, effects | C1 to C5, M1 |
| Collage, photo grid, mockup on a tilted surface | placement with perspective, masks | C2, C3 |
| One design, many pictures (a template run) | parameters, the scene as data, a command | C6, P1 |
| Animation frames, kinetic type | parameters as states, cached re-render | C6, C7 (video adds the clock) |
| Print-size piece | bands (B1), PDF and print colour (O1): built | B1, O1 |
| A reusable component (a badge in many scenes) | a scene as a layer source | C1, C6 |

**Takes** a scene and its inputs (pictures, drawings, fonts, maps, looks, values for its parameters). **Gives** a picture of
the scene at the size asked, and the description of every parameter. **Has no time:** a parameter is a state; whoever animates
it sets it per frame and renders again.

**Rust core, Python on top.** The engine is the pure-Rust crate `bullterrier-scene` (no PyO3, no numpy); the Python package is a
thin binding over it, as in image and vector, so a Rust program uses scenes without Python and both give the same picture.
The browser build is not part of the first plan (open question 1).

**Animation is states, not time.** A scene never knows a frame or a second. A parameter is a state; a caller (a loop, control
signals through video) sets it per frame and renders again. Per-layer caching (C7) and bands (B1) are therefore also what make
animation affordable, and video is scene plus the clock.

**Code first, JSON second.** The scene is built in code (Rust, Python); JSON is the same tree written down, never the other
way round.

**Looks and drawings are data at the boundary.** Scene links its own copy of the image and vector crates, so the Python facade
takes a look or a drawing by its serialised form (`to_dict()`, `to_json()`), not the other package's object, and a saved scene
embeds them. The Rust public API takes serialised data too: the pinned crates' types stay inside the two adapter files and out
of scene's semver. A raster is an array in code and a path with a digest in JSON.

## The shape of the module

One deep module, `Scene`, with a small interface, and one value, `Layer`. Both are immutable: every call returns a new one.

```
Scene::new(width, height)                 the design canvas: what a design pixel means
scene.add(layers)                         a layer is a value: source, placement, mask, blend, opacity, alpha, look
scene.with_values({address: value})       the only way to set a value (a number, colour, text or picture)
scene.render(size) -> array               the whole picture, float32 (height, width, 4), size the long side in pixels
scene.parameters()                        the description of every parameter, by address
```

A caller never sees what `render` does to answer: cut the canvas into bands, ask every layer for the rectangle it
contributes at the output scale, premultiply, resample, mask, blend, cache what did not change, convert back to straight
alpha. Those are hidden behind the one call; none is a public type.

- **A layer source is a trait with one question:** "draw this part of your box at this scale". Pictures, drawings,
  fields and nested scenes each answer it, with the raster and the part of the box it shows (a fill answers with one pixel,
  a picture without a look with its own). The scene never learns how: a picture is resampled, a drawing is rasterised by
  vector at that scale, a look runs through image. This is what makes bands possible.
- **One module per sibling crate** (`source/picture.rs` for image, `source/drawing.rs` for vector). Everything the scene knows
  about image's or vector's API is in those two files, so a change in either, or a different pinned version, touches one
  file. They do real work (the design-scale rule, the window and margin arithmetic); a file that only forwarded a call would
  not exist.
- **Layers, placements, masks and blends are plain values**, built with a chain of calls and held in the scene. No class per
  blend mode, no builder per layer kind.
- **Interface comments come first.** Each public item says what it promises (units, ranges, what an empty input gives), not
  how it works.

## Conventions

Image and vector each made sensible choices alone; a scene puts their output side by side, so the choices must meet.

1. **Design scale.** A look's `px` is image's design pixel (1/480 of the canvas's long side), whatever the size of the layer it
   belongs to. Grain of 2 px on a small inset photo is then as coarse as on the full-bleed photo behind it. Image derives the scale from the canvas it
   renders (the working picture's long side over 480) and carries it in the frame; a layer on a larger canvas needs image's
   `Options::with_canvas` (additive, asked for by C5).
2. **Blending in sRGB values**, as vector and the W3C Compositing and Blending spec do, so a drawing looks the same alone and
   in a scene. Linear-light blending is a later option, never a silent default.
3. **Straight alpha at every boundary**, premultiplied inside. Open: a look on an already transparent layer moves the colour
   but not the alpha until image carries alpha through effects that move pixels; until then a layer's look runs before its
   cut-out, and light a look adds past the shape shows only on a layer whose `alpha` is `light` ([api.md](api.md), "Looks").
4. **Room around a layer.** A shadow, glow or bloom reaches past the layer's box. The footprint an effect declares (how much
   neighbourhood it reads) also says how much room a layer needs, so nothing is clipped at its edge.

## Memory

Bands are built (batch B1; "Banded rendering" in [design-decisions.md](design-decisions.md) has the numbers): `Scene::render_bands`
draws the picture a band of rows at a time and `render_to_png`, `render_to_pdf` write it as it is drawn, so a print size past what
`render` draws is written. A banded render equals the whole render bit for bit, and `render` is one band.

A stack of full-size float pictures is the wrong way to render a poster: one 12-megapixel float32 RGB picture is about
144 MB, an A3 poster at 300 dpi about 200 MB a layer. So:

1. **Render in bands.** The canvas is cut into rows, each band composed through every layer, then the next. A band is a window of
   rows on the one plane every layer is drawn for (`Target::window`): the output, a group's inner scene and a mask's plane all take
   one, and a pixel is what it is whatever rows are drawn with it, so the bands put together are the whole.
2. **What is drawn by band and what whole.** A picture with no look hands its own pixels to the warp, which reads the rows a band
   needs (with a look, it is resized for those rows); a flat colour is one pixel; a group is drawn for the rows of its raster the band reads (`warp::rows_read`); a mask plane
   is drawn for its band and the reach of its blur (the box passes of the blur are aligned so its sums add in the same order). A
   layer with a look is drawn for the rows a band reads and the margin its looks reach (image's `Recipe::band_margin`, run with
   `Options::with_window`: a window of the whole raster, so each pixel is the whole render's) when its source can draw rows, the layer
   is not light, and image can band the recipe; else **whole** once and cut (image's whole-picture effects, a layer whose raster is not
   the canvas's size, looks of a recipe image does not clear to the bit: a blur summed by a Fourier transform, `grain` clumped past about 8 working pixels). A drawing is drawn for the rows a band reads (vector's window render is the crop of its
   whole render, byte for byte). A plugin kind is drawn whole once, unless it says its pixels depend on their place alone
   (`LayerKind::by_position`): then it is asked for the rows a band reads. These are the memory floor of a banded render.
3. **Finished layers are held as float32.** Decision 2 of the roadmap, settled at B1 with the peak measurements.
4. **Vector draws a window**, exact to the byte, so a drawing is drawn by band. A window vector refuses (a side past 65,000 pixels, 2^27 in all) is a raster its whole render refuses too: there is no whole fallback.
5. **Design pixels belong to the whole canvas,** not the band, so a banded render equals a whole one.

The test that decides it: a banded render equals the whole render for every example scene at several band heights, and the peak of a
15-layer A3 poster is measured in both.

## Why a package of its own

| Option | Why not |
|---|---|
| Inside image | Image may not depend on vector. Taking a drawing as a ready picture makes the caller pick the size before image can draw it, which breaks "the same look at any resolution". Image would stop being "one picture in, one picture out", and video would still need its own layer model. |
| Inside vector | Vector is geometry and has chosen never to link a raster library. |
| Inside video | Video also carries decoding, encoding, sound and a timeline a still has no use for; a poster would wait for the largest package. |
| An app or script outside every package | Works once; every user rewrites the same layer logic and video still needs its own. |
| **A package below video** | Chosen: image and vector stay untouched, video becomes scene plus time, and stills ship first. |

What it gives beyond posters: video gets smaller (a video frame is a scene at one moment, and footage enters as a layer the
caller hands in); an animated poster and a still are the same scene file; compositing is specified and tested once against the
W3C spec; a draft equals the final render; a server renders one scene file with no Python; one template filled with a photo and
a title per run is image's batch idea one level up; memory discipline lives in one place; and image and vector are tested
together on real pieces long before footage exists.

Costs: a sixth repository to run; blend modes in three places, each tested against the same spec; the line to video (anything
needing time, footage or sound is video's) has to be held; and image and vector are moving crates, so scene pins them.

## Dependencies on the siblings

Image and vector only. Scene does not depend on control (`docs/design-decisions.md`): a value is a plain number.

Image and vector are not published; scene takes them as git dependencies pinned to a revision and bumps the pin in the batch
that needs a newer one. The first batch to need each is in the roadmap table.

## Open questions

1. Whether a browser build is wanted for scene, and what it asks of image's crate.
2. ~~How a scene refers to image looks and vector drawings.~~ Settled in A1: embedded data is the default and the saved form; a
   path is read-only convenience ([api.md](api.md), "Data in").
3. Print colour (CMYK, ICC, bleed, PDF) is output and belongs here: built as a raster PDF (O1); a PDF that keeps vectors is not planned.
4. ~~Command line in Rust or Python.~~ Settled in A1: a Rust command line that the Python console script runs, as image does (P1).
5. Stamped brush marks along a path (spray, scribbles) are raster, so a layer type here, with vector giving the path.
