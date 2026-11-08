# Design

What bullterrier-scene is and how it is built. Reasons for single choices: [design-decisions.md](design-decisions.md).
The batches, in order: [roadmap.md](roadmap.md). Status: draft for review, 2026-11-08.

## What it is

A scene is a canvas and a stack of layers, drawn into one picture at any size. A layer is a source (a picture, a vector
drawing, a flat colour or gradient, another scene, a picture the caller hands in each time) plus how it sits on the layers
below it: its place, its mask, its blend mode, its opacity and the effects it gets. Layers can be grouped, and a group
blends as one.

| Responsible for | Not responsible for |
|---|---|
| The canvas and the stack of layers, in groups | Effects on one picture (grade, halftone, glitch) → image |
| Placing a layer: position, rotation, scale, skew, perspective, anchor, fit | Shapes, type, booleans, SVG → vector |
| Masks: from a shape or text, an alpha, luminance, a given map, a subject cut-out; soft or hard; inverted | Finding a subject or a depth map → image |
| Blend modes (the W3C set), opacity, groups that blend as one | Values changing over time → control |
| An effect chain on a layer or a group, applied at the right size | Footage, a timeline, sound, encoding → video |
| Describing every parameter of the scene (name, unit, range, meaning) | An editor or painting tools |
| Rendering in bands; caching per layer | |
| Output: a picture, PNG, later PDF and print colour | |

**Takes** a scene and its inputs (pictures, drawings, fonts, maps, looks, values for its parameters). **Gives** a picture of
the scene at the size asked, and the description of every parameter. **Has no time:** a parameter is a state; whoever animates
it sets it per frame and renders again.

**Code first, JSON second.** The scene is built in code (Rust, Python); JSON is the same tree written down, never the other
way round.

## The shape of the module

One deep module, `Scene`, with a small interface:

```
Scene::new(width, height)                 the design canvas: what a design pixel means
scene.add(layer)                          a layer is a value: source, placement, mask, blend, opacity, look
scene.render(size, values) -> Image       the whole picture; values set the scene's parameters
scene.parameters()                        the description of every parameter
```

A caller never sees what `render` does to answer: cut the canvas into bands, ask every layer for the rectangle it
contributes at the output scale, premultiply, resample, mask, blend, cache what did not change, convert back to straight
alpha. Those are hidden behind the one call; none is a public type.

- **A layer source is a trait with one question:** "draw this rectangle of the canvas at this scale". Pictures, drawings,
  fields and nested scenes each answer it. The scene never learns how: a picture is resampled, a drawing is rasterised by
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

1. **Design scale.** A `px` value in a scene means a pixel of the scene's design canvas, whatever the size of the layer it
   belongs to. Grain of 2 px on a small inset photo is then as coarse as on the full-bleed photo behind it. Image today
   derives the scale from each picture's own long side; the scene needs it from the caller (a pending change in image).
2. **Blending in sRGB values**, as vector and the W3C Compositing and Blending spec do, so a drawing looks the same alone and
   in a scene. Linear-light blending is a later option, never a silent default.
3. **Straight alpha at every boundary**, premultiplied inside. Open: a look on an already transparent layer moves the colour
   but not the alpha until image carries alpha through effects that move pixels; until then a layer's look runs before its
   cut-out, or on a group with an opaque ground.
4. **Room around a layer.** A shadow, glow or bloom reaches past the layer's box. The footprint an effect declares (how much
   neighbourhood it reads) also says how much room a layer needs, so nothing is clipped at its edge.

## Memory

A stack of full-size float pictures is the wrong way to render a poster: one 12-megapixel float32 RGB picture is about
144 MB, an A3 poster at 300 dpi about 200 MB a layer. So:

1. **Render in bands.** Cut the canvas into rows, compose each band through every layer, then the next. Peak memory is a few
   bands, not a few canvases, and does not grow with the number of layers.
2. **Each effect declares its footprint:** nothing around a pixel, a radius, or the whole picture. Bands are cut with that
   margin; a whole-picture effect falls back to one full picture, and only that layer pays.
   A footprint is honest only if the effect's output at a pixel depends on absolute position and canvas size, never on a
   sequential stream: an effect that draws noise from one seeded stream (grain, dust, scratches) is whole-picture until its
   noise is position-hashed. Footprint also depends on the parameters (a blur's radius).
3. **Finished layers are held compact** (8 or 16 bits), not float32; float is for the effect that is running.
4. **Vector draws a window**, so a band of a drawing never needs the whole raster.
5. **Design pixels belong to the whole canvas,** not the band, so a banded render equals a whole one.

The test that decides it: a banded render equals the whole render, and the peak of a 15-layer A3 poster is measured in both
before any promise is made (batch B1). Needs from image (footprint, banded render, a design scale from the caller) and from
vector (a window render) are in [roadmap.md](roadmap.md).

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

Image and vector are not published; scene takes them as git dependencies pinned to a revision and bumps the pin in the batch
that needs a newer one. The first batch to need each is in the roadmap table.

## Open questions

1. Whether a browser build is wanted for scene, and what it asks of image's crate.
2. How a scene refers to image looks and vector drawings: embedded data or a file.
3. Print colour (CMYK, ICC, bleed, PDF) is output and belongs here; when to build it is open.
4. Stamped brush marks along a path (spray, scribbles) are raster, so a layer type here, with vector giving the path.
