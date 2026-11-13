# The code API

Status: A1, designed from the example programs in [examples/](examples/); C1 (the walking skeleton), C2a (`fit` and the
affine `place`) and C2b (`perspective`) are built, the rest is not. What they make real, and where they differ from the contract below, is "What is
built". Every other signature is what batches C3 to C7 make real, and each example is the acceptance material of the batch
that makes it run. Reasons:
[design-decisions.md](design-decisions.md). The JSON form of a scene is "Saved scene" below.

Signatures are Python's; "Languages" gives the Rust form. The library is imported under the alias `bts` in every language.

```python
import bullterrier_scene as bts
```

## What is built

**C1.** Python: `Scene(width, height)`, `add`, `layers`, `width`, `height`, `render`; `Layer.picture` (an array or a `Path`),
`Layer.drawing`, `Layer.fill`, `named`, `opacity`, `look`; `InvalidError`, `FormatError`, `ReadError`; `MAX_LAYERS`. Rust:
`Scene::{new, add, layers, width, height, render}`, `Layer::{picture, picture_file, drawing, fill, named, opacity, look,
look_file}` (`look` and `drawing` take the document as text), `Raster::rgba`, `Error`, `MAX_LAYERS`.

**C2a** adds `Layer.fit`, `Layer.place` (Python) and `Layer::{fit, place}`, `Fit`, `Place`, `Deg` (Rust: `layer.fit([x, y, width,
height], Fit::Cover, [0.5, 0.5])?` takes all three, `layer.place(Place::new().dx(100.0).rotate(Deg(-4.0)))?` sets the fields
named).

**C2b** adds `Layer.perspective(corners)` (Python, four (x, y) pairs) and `Layer::perspective([[f64; 2]; 4])` (Rust). References
(`Layer.of`) are not built. Nothing else below exists yet. Each call is as written
below, except:

| Where | Built | Changes with |
|---|---|---|
| Boxes | A picture's box is its pixels and a drawing's its frame, one design pixel each, at the canvas origin; a fill covers the canvas; `fit` and `place` move them. What lies outside the canvas is cut. The picture covers the canvas exactly: the long axis has exactly `size / long side` output pixels per design pixel; the short axis has its length rounded up to whole pixels and the picture is stretched onto them, so there a design pixel is `rows / height` (or `columns / width`) output pixels, within one pixel over the whole canvas of `size / long side`. That is what lets a layer that covers the canvas cover it whole at any size. **Limit:** on a thin canvas the design pixel is visibly not square (a canvas of 100 by 3 at `size` 7 is 7 by 1 pixels: 0.07 output pixels along x and 1/3 along y), and a turn or a skew of a layer on it is sheared by that ratio; draw such a canvas at a size where its short side is whole. A box that ends between two output pixels covers the pixel it ends in in part | |
| Resampling | One warp per layer ([design-decisions.md](design-decisions.md), "Placement"): bilinear on premultiplied colour; where the map shrinks the picture, the average of what the output pixel's footprint covers (an exact area average where the map keeps the axes, a lattice of taps over the footprint elsewhere), at any skew; no negative weight (a bicubic upgrade must restore a clamp); nothing outside the fitted box is drawn and an edge between two pixels is soft by coverage; a move by whole pixels, a quarter turn and a flip are exact copies | |
| `fit`, `place` | `anchor` outside 0 to 1, and `dx` or `dy` outside -1e6 to 1e6 (the range of the parameters), are `InvalidError` naming the field. A call that sets a number the call refuses leaves the layer as it was. `fit` is resolved when the scene renders | |
| `perspective` | Corners beyond -1e6 to 1e6 or not finite, and corners that are no strictly convex quadrilateral, are `InvalidError` naming `corners`; a mirrored (counter-clockwise) one is accepted. A layer under one is drawn at the largest scale the quadrilateral gives any corner of its window along each axis, cut down by one factor to the 2^27 pixels of a layer where it would pass them (not refused: the near side loses sharpness, which window drawing, B1, lifts). A parallelogram is affine and takes the exact path: an axis-aligned one is what `place` makes, bit for bit | |
| A look | Only a picture layer takes one (a fill or a drawing is `InvalidError` naming `look`). It runs on the fitted raster at the size the layer has: the source drawn at `output scale x fit factor x scale` (see "How a layer is drawn"), on its straight colour with the alpha left as it is: the contract's "colour over black" for an opaque picture, which is all C1 tests. A look's `px` are image's own, a pixel of the layer's drawn picture whose long side is 480, not yet pixels of the canvas; the tests keep to canvas-sized pictures so that C5 does not rewrite them. Image refuses a look on a picture of more than 2^26 pixels, in its words | C5 |
| `format` of a look | `Recipe.to_dict()` writes none, and image's reader refuses a top-level `format`. A document that carries one is checked against a copy of image's list (1, 2, 3; image keeps it private, a test fails when a bump of the pin changes it) and the key is removed. The refusal names the version found and the versions read, not the crate's version, which neither crate publishes | image: a public list and version |
| A drawing | `inputs` (font bytes) is not built: no layer needs a font yet. A document vector cannot read is `FormatError` under `/drawing` in vector's words, a version skew included, without the crate's version. Vector draws the whole frame, not a part (B1) | C6, text layers |
| Errors | The address of an error of a layer in a scene starts with its name (`photo.look`), a layer without one `layer1`; at the call that made it, before the layer has a place, it is the field (`look`) | |
| Python paths | `Layer.picture(Path)` is read at `render`; `Layer.drawing(Path)` and `Layer.look(Path)` at the call | |
| Equality | Not built: `==` on a scene or layer is identity in Python, and the Rust types have no `PartialEq` | C6 |

**For C5 and later** (what the C1 review found): a look lives in the picture source, so C5, which gives every layer looks,
moves it to the layer and needs a third adapter file for image; the `Source` trait and the `Layer` fields are `pub(crate)`, and
a plugin layer (C7) needs a public trait. The trait is now "draw this part of your box at this scale" and answers with the
raster and the part of the box it shows, so a fill answers with one pixel and a picture without a look with its own pixels.

**For C5 and later, from the C2a review** (not built): `Layout::warp` clips by `window == covers`, so a look that grows the
raster past the box (its room) would be clipped away: `Drawn` needs a visible rectangle of its own, apart from what it covers.
`Layout` holds one `Placed`, and the extra `place` of C4's `Layer.of` needs a composite of two. `Source` has no identity, which
C7's cache key needs.

**Limit: alpha.** The alpha of an interior pixel of an opaque layer is exactly 1 after a move, a quarter turn, a flip, any
turn, a perspective or an enlargement; after the exact area average of a map that keeps the axes (a reduction by a scale, a flip
or a quarter turn) it may be 1 - 1e-7, because the weights of a span in 32-bit floats sum to a step under 1.

**Limits, measured** (release build, 2026-11-13, arm64 Mac, peak resident memory of a whole render):

| Render | Peak |
|---|---|
| nothing drawn, 2^27 pixels (16,384 by 8,192) | 2.1 GB |
| one fill, 2^27 pixels, 0.3 s | 2.15 GB (C1: 4.3 GB, measured again by the reviewer; the earlier 2.8 GB was wrong) |
| one canvas-sized picture, 2^26 pixels (8,192 squared), 0.15 s | 2.15 GB; with a look (3.1 s) 4.2 GB (C1, not measured again); turned 17 degrees and scaled 0.9 (1.1 s) 2.9 GB |
| one canvas-sized picture, 11,585 squared (2^27 pixels), 0.5 s | 4.3 GB (an unplaced layer is blended as it is, not copied; C1's own 5.5 GB was not measured again) |
| 1,000 fills on 1,024 by 1,024, 0.67 s | 19 MB (C1: 0.89 s, 36 MB) |
| C2a: one 12 megapixel picture on a 12 megapixel canvas: unplaced 0.03 s, 386 MB; moved 0.05 s, 386 MB; scaled 0.9 and moved by half a pixel 0.12 s, 520 MB; turned 17 degrees and scaled 0.9, 0.19 s, 527 MB | |
| C2a: the same picture cut by `cover` into 720 by 1,280, 0.03 s | 326 MB |
| C2b: the same picture turned 17 degrees and scaled 0.9, 0.46 s (C2a: 0.19 s, 530 MB); skewed 89 degrees, 0.22 s; in perspective with a far side a tenth as wide, 0.50 s | 386 MB each |

`MAX_LAYERS` (1,000) and the 2^27 pixels stay. The 2^27 applies to the output and to every layer's drawn size (its box at
the scale it is drawn at, `ceil(width x scale)` by `ceil(height x scale)`, the scale being `output scale x fit factor x
scale`: a layer that is large on a small canvas is `InvalidError` naming the layer and the size, before it is drawn). A large
`scale` on a big layer meets it even where only a part of the layer shows: drawing a window (B1) lifts that, and it is not
worked around. A render holds the picture, the one layer being drawn and its working copies, not one picture per layer, so peak
memory does not grow with the layers. A picture at the limit takes 5.5 GB; there is no promise
that a machine has it, and bands (B1) are what lift it. `MAX_DEPTH` waits for scenes as layers (C4).

## Examples

Each program is a `build(...)` function that returns a `Scene` from its inputs (a photo array, a matte, a path), and a
`__main__` block that loads real files and renders. A test calls `build` with synthetic inputs and needs no model.

| Program | Shows |
|---|---|
| [c1_first_picture.py](examples/c1_first_picture.py) | C1 (runs now: `uv run poe look`): a synthetic photo with one image look and a vector headline over it, rendered at two sizes |
| [c2_prints.py](examples/c2_prints.py) | C2a (runs now: `uv run poe look`): a pile of synthetic snapshots, each `fit` (cover, contain, stretch) and placed (turned, skewed, mirrored, scaled), and one tall photo cut three ways into a 9:16 frame |
| [c2b_perspective.py](examples/c2b_perspective.py) | C2b (runs now: `uv run poe look`): a checker road receding to a horizon, where its checks are smaller than a pixel and fade to grey, and two billboards on quadrilaterals, one mirrored |
| [poster_two_ink.py](examples/poster_two_ink.py) | M1: a photo in two inks clipped into a polygon, type behind a cut-out subject, flat shapes, grain, three sizes |
| [cover_template.py](examples/cover_template.py), [.rs](examples/cover_template.rs) | C6: one scene, a photo, a title and a colour set by `with_values`; saved as JSON and read back; six covers |
| [frame_sequence.py](examples/frame_sequence.py) | C7: a headline sliding in over 24 frames, one value set per frame |
| [cache_probe.py](examples/cache_probe.py) | C7: a layer that did not change is not drawn again, observed through a counting plugin layer |
| [composed_world.py](examples/composed_world.py) | M2: a cut-out dog on a grid floor under a striped sun; shadow and reflection by reference, a glow on the sun only |
| [mill.py](examples/mill.py) | M3: a night-graded photo, windows and sign lit as layers, a logo with its own glow, a 9:16 crop by `fit`, every moving value per frame |
| [badge_scene_in_scene.py](examples/badge_scene_in_scene.py) | a scene used as a layer, twice, with different words |
| [chrome_type.py](examples/chrome_type.py) | C5: chrome type: a vector headline lit by image's `liquid_metal` and `relight`, correct at two sizes; an emboss shadow as its own layer |
| [wow_poster.py](examples/wow_poster.py) | the final poster: layered type, cut-out, neon, halftone, glow on chosen layers, mixed blends |

## Five ideas

**`Scene`** is a canvas (width and height in design pixels) and a stack of layers, bottom first. It is an immutable value:
every call returns a new scene, the old one is unchanged and can be used again. It renders at any size: `scene.render(1080)`.

**`Layer`** is an immutable value too: a source (a picture, a drawing, type, a flat colour, another scene, or another layer
of the same scene), how it sits (`fit`, `place`, `perspective`), how it joins what is below (`opacity`, `blend`, `as_light`),
what hides parts of it (`mask`) and what changes it (`look`). Every call returns a new layer. There is no class per kind of
layer, blend mode or mask: a layer is one type. A scene used as a layer is a group that blends as one; `Layer.of("name")`
draws a layer of the same scene again, with its own looks and masks (a glow, a shadow, a reflection) and none of its
parameters.

**Looks and drawings enter as data.** A look is an image `Recipe`, a drawing a vector `Drawing`. Scene links its own copy of
both crates, so it takes what you hold by its serialised form (`to_dict()` or `to_json()`), never the Python object. A saved
scene embeds them. Scene imports neither package in Python.

**Values and parameters.** Every number, colour, text, picture and choice of a scene has an address: `title.opacity`,
`photo.picture`, `logo.drawing.ring_on`, `windows.look.swell.amount`. `scene.parameters()` describes them,
`scene.with_values({address: value})` sets them and returns a new scene, and nothing else sets a value (`render` takes none).
A frame of an animation is `scene.with_values({...}).render(1080)`; the clock is the caller's.

**One picture out.** `scene.render(size)` is a float32 array (height, width, 4), straight alpha, what `imageio.save`
accepts. Bands, caches, resampling and premultiplication are behind that call.

## Scene

| Member | Signature | Promise | Errors |
|---|---|---|---|
| `Scene` | `Scene(width, height)` | an empty canvas of `width` by `height` design pixels: whole numbers, 1 to 100,000. A design pixel is what a `px` value means everywhere in the scene | `InvalidError` |
| `add` | `add(*layers)` | the scene with `layers` on top of its stack, first argument lowest | `InvalidError`: a name used twice (naming both layers); a `Layer.of` that names no layer of the scene or its own `add` call, or that leads back to itself; more than `MAX_LAYERS` (1,000) layers; scenes nested more than `MAX_DEPTH` (16) deep. `TypeError`: a non-`Layer` (wrap a scene with `Layer.scene`) |
| `layers` | property | the layers, bottom first, a tuple | |
| `width`, `height` | properties | the canvas | |
| `parameters` | `parameters()` | every parameter, by address, in layer order ("Values") | |
| `with_values` | `with_values(values)` | the scene with `values` (a mapping from address to value) set | `InvalidError`: an address the scene lacks (the message lists the valid ones and the nearest), a value outside its range or of the wrong kind (the address, the rule and the value) |
| `render` | `render(size)` | the picture ("Output") | `InvalidError`, `ReadError` |
| `to_json` | `to_json()` | the scene as JSON text, self-contained ("Saved scene") | `InvalidError`: a picture or a map that is an array (it has no file). `ReadError`: a file whose digest is to be written cannot be read |
| `Scene.from_json` | `Scene.from_json(text, *, base=None)` | the scene back; a relative path in it resolves against folder `base` (a `Path`; default: the working folder). Reads no picture, font or map: those are read at `render` | `FormatError` |

Scenes and layers are equal when they hold the same data (a picture by its content digest, a drawing and a look by their
serialised form). Equal scenes with equal values render equal pictures.

## Layer

### How a layer is drawn

A layer is drawn at the scale it will have (see "The scale a layer is drawn at"), looked at that size, and then put on the
canvas as a finished raster by one warp. One order, whatever the order of the calls; the calls matter only in the order of
looks and in the numbering of masks (`mask`, `mask#2`). Each stage names its space: the layer's space is its own fitted
raster, the canvas is the scene's.

1. **Source.** The layer's picture is drawn at its box (layer space).
2. **Fit.** `fit` scales and crops it into its box; without `fit` it stays one design pixel per source pixel.
3. **Looks,** in call order, on the fitted raster (layer space). A look's `px` is a pixel of the canvas as the layer is
   fitted, before `place`. A look that has a centre or an edge (a vignette, a warp) takes it from the fitted box. Looks see
   the layer's colour over black. **Room:** the fitted box grows by the look's footprint, so a halo or a bloom is not cut at
   the box's edge; the room is transparent unless the layer is light (stage 7).
4. **Own-map masks** (`mask("name")`): layer space; the map is cut and fitted with the picture.
5. **Place.** The finished raster is flipped, stretched along y by `aspect`, skewed and rotated about `anchor`, then moved by
   `dx`, `dy` (or `perspective`), by one warp. Its `scale` is in the drawing: the source drew at that scale and the look ran
   at that size, so a `scale` of 1.32 scales a halo with it.
6. **Layer masks** (`mask(Layer)`): canvas space, multiplied.
7. **Light** (`as_light`): the layer's alpha is made from the brightness of its colour.
8. **Opacity.**
9. **Blend** over what is below.

A `Layer.of` inherits stages 1 to 5 of the layer it names and owns stages 6 to 9 ("Reference").

**The scale a layer is drawn at.** A source draws the part of its box that `fit` shows (all of it without `fit`, the cut part
for cover) at `output scale x fit factor x place.scale`, in output pixels per design pixel along each axis: a vector headline
at `scale=3` is sharp, not an enlarged raster, and its look runs at the size the layer has. `aspect`, flip, skew, rotate,
`dx`, `dy` and `perspective` are not in it: they are the one warp of the finished raster onto the canvas, which samples
each output pixel's centre from it (bilinear, averaged by area along any axis the warp shrinks), draws nothing outside the
fitted box, and leaves a pixel an edge crosses covered in part. For a picture with no look the source hands over its own
pixels, so source, `fit` and `place` are one warp with no resample between them.

- **Perspective.** The source draws at the largest scale the quadrilateral gives any corner of the window along each axis, so the
  near side is sharp, and the warp filters the far side by each pixel's own footprint (the next point). If that would pass the
  2^27 pixels of a layer it draws at a smaller scale that does not, and the render stays valid; the near side then loses
  sharpness. Window drawing (B1) lifts that.
- **Footprint.** Where the warp shrinks the picture each output pixel is the average of what its footprint covers: an exact area
  average where the map keeps the raster's axes (a scale, a flip, a quarter turn), a lattice of taps over the footprint
  otherwise (a turn, a skew, a perspective), at any skew. Where the footprint is no larger than a pixel it is plain bilinear.
- **Cost.** An animated `scale` draws the source and runs its look again every frame. `dx`, `dy`, `rotate`, `skew`, `flip`
  and `aspect` only warp a raster that did not change, which is what C7 caches.
- **Zoom limit.** A large `scale` on a big layer meets the 2^27 drawn-size limit even where only a part shows; drawing a window
  of the layer (B1) lifts it.
- **After C5.** Looks take design pixels of the canvas. The design scale handed to image is then `canvas scale x place.scale`
  (not the fit factor alone), so a halo still follows `scale`.
- **Fit.** The window `fit` shows is computed from the source's real size when the scene renders. `stretch` has a scale for
  each axis. The scale that binds (width or height) ends at the box exactly, so a layer fitted into a box of its own shape is
  not touched.

### Sources

| Source | Signature | Promise |
|---|---|---|
| picture | `Layer.picture(source=None, *, maps=None)` | `source`: an array (height, width, 3 or 4; `uint8`, `uint16` or floats 0..1; 4 channels are straight alpha), a `pathlib.Path` (read as `imageio.load_picture` reads: upright, sRGB, alpha kept), or `None`: a hole the caller fills with `with_values` before `render`. `maps`: the picture's named maps, each `None` (a hole), a float32 (height, width) array 0..1, or a `Path` to a grey image (read as `imageio.load_map` reads). A map is a parameter of kind `picture`, `<layer>.map.<name>`, set by `with_values` like the picture: a footage frame with its matte, or a template with a cut-out, is driven by `with_values`. A map of another size than its picture is `InvalidError` at render naming both sizes. A look sees the maps its recipe uses (`subject`, `depth`, a `given` region); the others serve `mask("name")`. A `str` source is `TypeError` (pass `Path(...)`). Its box is its pixels, one design pixel each, at the canvas origin. An array from `imageio.load` has no alpha (image puts it over white): give a transparent PNG as a `Path` or `imageio.load_picture` |
| drawing | `Layer.drawing(source, *, inputs=None)` | `source`: a vector `Drawing` (anything with `to_json()`), its JSON text (a `str`) or dict, or a `Path` to a drawing file. Its box is the drawing's frame (`size`) at the canvas origin: a drawing without a frame is refused. `inputs`: the bytes or `Path` of each font or map file the drawing names, by name (vector's `Drawing.from_json(inputs=)`); a bundled font needs none |
| text | `Layer.text(content, *, font, color="#000000", **text_options)` | scene owns `content`, `font` and `color`. `font`: a bundled font's name (a `str`) or a `Path` to a font file. `text_options` are `btvec.text`'s (`size`, `axes`, `tracking`, `leading`, `align`, `width`, `fit`, `features`...) as plain data, validated by the linked vector crate at the call; scene restates none of vector's defaults. `width` and `fit` mean vector's: the width the text wraps or fits to, not a placement (`fit(...)` on the layer is the placement). `anchor` and `inside` are refused, and so are expressions and Params: scene places the text, and per-letter numbers are a `Layer.drawing`. Its origin is the text's anchor, the left end, middle or right end of the first baseline by `align`: `place(dx=100, dy=420)` puts it there. Its box is the glyphs' bounds |
| fill | `Layer.fill(color)` | a flat `"#rrggbb"` or `"#rrggbbaa"` over the canvas box; `fit` makes it a band, a mask makes it a shape. Gradients are vector's: a rectangle filled with `btvec.linear_gradient` |
| scene | `Layer.scene(inner)` | another scene as a group: its canvas is its box; it blends, fades, masks and takes a look as one picture. `inner`'s parameters are addressed under the layer: `badge.scene.label.text` |
| reference | `Layer.of(name)` | see "Reference" |
| plugin | `Layer.make(kind, **options)` | a layer of a registered kind, saved by `kind` and `options`. A kind is registered by name in the entry-point group `bullterrier_scene.layers` and modifies nothing in the package; it is asked for one rectangle of the canvas, `(x, y, width, height)` in design pixels, at a scale in output pixels per design pixel, and returns a float32 RGBA array of `ceil(height * scale)` rows by `ceil(width * scale)` columns. `bts.register_layer(kind, draw)` registers one in process (a test's way). Arrives with C7: the cache test counts the `draw` calls of such a kind |

Things that must move together (a logo and its glow, a dog and its shadow) are one layer and its references. When a group
of layers must move together, make them one inner scene and use it with `Layer.scene`.

### Reference

`Layer.of(name)` draws the layer called `name` of the same scene, again. It inherits stages 1 to 5 of that layer (its
source, `fit`, looks, own-map masks and `place`) with that layer's own values, so its source and placement parameters stay
at the named layer's address: `logo.drawing.ring_on` is set once and the logo's glow follows. It does not inherit that
layer's layer masks, `as_light`, opacity or blend: those are scene composition, like opacity, and the reference owns its
own. It adds its own looks after the named layer's, its own-map masks, and an extra `place` applied after the named layer's
(about its own `anchor`, as fractions of the bounding rectangle of the placed quadrilateral: the named layer's box after its
`place`, which a rotated layer makes a tilted quadrilateral). It has no source, no `fit` and no `perspective`
(`InvalidError`).

```python
logo_glow = bts.Layer.of("logo").named("logo_glow").look(halo).as_light().blend("screen")
shadow = bts.Layer.fill("#000000").mask(bts.Layer.of("dog").place(aspect=0.12, anchor=(0.5, 1.0)), feather=12)
```

A reference names a layer of its own scene that is already added or is in the same `add` call; it may name another
reference. A chain of references that leads back to itself is `InvalidError` at `add`. Its own parameters are the fields of
any layer (`opacity`, `blend`, `light`, the placement fields) and its looks: `logo_glow.look.halo.amount`.

### Placement

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `named` | `named(name)` | the layer called `name`: letters, digits and `_`, starting with a letter, unique in its scene. A layer without a name is `layer1`, `layer2`... by its place in the scene, which shifts when a layer is inserted: name every layer you set values on or refer to. `layer<n>` is refused as a name for any other place | `InvalidError` |
| `fit` | `fit(box, how="cover", *, align=(0.5, 0.5))` | the layer's box fitted into `box` = `(x, y, width, height)`, design pixels. `"cover"`: scaled, aspect kept, to fill `box`, the overflow cut off at `box`'s edge; `"contain"`: scaled to lie wholly inside; `"stretch"`: `box` exactly, aspect not kept. `align` (fractions 0..1 of the layer's box) says which part stays when cover cuts and where a contained layer sits in the slack. Resolved at render from the picture's real size, so a hole's picture of any size fills the same box; cover is the delivery crop. `place` acts on the fitted box: the cut `box` for cover, the contained rectangle for contain. The cut is a hard clip of the layer's own raster, not a mask. Not a parameter | `InvalidError`: `how` another word, `box` with a size not above 0 or a number that is not finite, `align` outside 0..1 |
| `place` | `place(*, dx=None, dy=None, scale=None, aspect=None, rotate=None, skew=None, flip=None, anchor=None)` | sets the fields it names and keeps the others; `flip=(x, y)` sets `flip_x` and `flip_y`, `skew=(x, y)` sets `skew_x` and `skew_y`. The finished raster is flipped by `flip` (`(x, y)` booleans), scaled by `scale` (uniform) and by `aspect` on y (the height ratio: y scale is `scale` times `aspect`, x scale is `scale`), skewed by `skew` (degrees: `skew_x`, then `skew_y`, one after the other, so the transform's determinant stays 1 and the bound below is enough) and rotated by `rotate` (degrees clockwise, any size, wrapping), all about `anchor` (fractions 0..1 of the fitted box, default the centre), then moved by `dx`, `dy` design pixels (not turned with the layer). Defaults: no change. A rotation by a multiple of 90 degrees, a flip and a move by whole output pixels are exact copies of the pixels (about an anchor on the pixel grid) | `InvalidError`: `scale` or `aspect` outside 0.001..1000, a skew outside -89..89, `dx` or `dy` outside -1e6..1e6, an `anchor` outside 0..1, a non-finite number |
| `perspective` | `perspective(corners)` | the four corners of the fitted box land on `corners` (top left, top right, bottom right, bottom left; design pixels of the canvas), by the homography of the box to the quadrilateral, replacing `place`'s transform (the last of the two called wins; `fit` comes first, so the corners are the fitted box's). The quadrilateral must be strictly convex, which is what a homography maps a rectangle to: all four cross products of consecutive edges above 0, or all below. One that turns counter-clockwise is accepted: it is the box seen from behind, a mirror (the picture of a clockwise quadrilateral with its corners left and right swapped). Not a parameter | `InvalidError` naming `corners`: a number beyond -1e6..1e6 or not finite; a dart (a corner inside the others), a bowtie (edges that cross), three corners on a line, a corner twice |

**`place` and `perspective` are one state.** A layer has one placement: the fields of `place`, or the quadrilateral of
`perspective`. `perspective` replaces whatever `place` said. `place` after a `perspective` replaces the quadrilateral and starts
from the default fields plus the ones it names: the fields of a `place` made before the `perspective` are not brought back.

`place`'s fields are parameters: `dx`, `dy` (px), `scale`, `aspect` (ratio), `rotate` (degrees, cyclic), `skew_x`, `skew_y`
(degrees), `flip_x`, `flip_y` (bool). `scale` is the zoom of an animation; a flat shadow is `aspect=0.12`. A layer scaled by
`place` is scaled with its looks: a halo grows with it, and a draft and a print differ in detail only. "How a layer is drawn"
says at what scale it is drawn, and what an animated `scale` costs.

### Joining the layers below

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `opacity` | `opacity(value)` | sets the layer's opacity to `value`, 0..1 (default 1). A group fades as one | `InvalidError`: outside 0..1 |
| `blend` | `blend(mode)` | how the layer joins what is below, the W3C compositing modes by their CSS names: `"normal"` (default), `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`, `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`. Blended in sRGB values, as vector and a browser do. A look's own `blend_mode` (image's spelling: `soft_light`) is inside the look and stays image's | `InvalidError`: another word |
| `as_light` | `as_light(on=True)` | makes the layer light, or not (default: not). A light layer's premultiplied colour is its look's colour over black, `c`, and its alpha is the largest of R, G, B of `c`; its straight colour is `c / alpha` (0 where alpha is 0). So a glow shows beyond the shape; black adds nothing, a `screen` of the layer over black gives `c` exactly, and white is opaque white. Masks scale it. A layer that is not light keeps its source's alpha, after its masks, and a halo or shadow its look adds outside the shape is cut | `TypeError`: not a `bool` |

### Masks

`mask(by, *, use="alpha", feather=0, invert=False)` multiplies the layer's alpha by a mask. Calling it again multiplies
another mask in.

| Argument | Meaning |
|---|---|
| `by` | a `Layer`: drawn on the canvas as its own place says (stage 6), read as `use` says; it may be `Layer.of("subject")`, which follows `subject`'s parameters. Or a `str` naming one of this layer's own `maps`: a map of its picture, cut and fitted with it (stage 4). A reference reads the `maps` of the layer it names |
| `use` | `"alpha"` or `"luminance"` (0.2126 R + 0.7152 G + 0.0722 B of the sRGB values, times the alpha: white keeps, black hides), for a `Layer`, read from the mask layer's colour as drawn (a `Layer.of` mask: the named layer's picture with the reference's own looks); a map is its own values |
| `feather` | the standard deviation, in design pixels, of a Gaussian blur of the mask's edge; 0 is hard |
| `invert` | 1 minus the mask (after the feather): type that passes behind a subject is masked by the inverted subject |

A mask layer is owned by the layer it masks and is outside the scene's names: it is addressed `<layer>.mask.<field>`, the
second `mask#2` (the only thing that depends on the order of the calls). A mask layer's alpha includes its own masks, not
its looks. A `Layer.of` mask has the fields of a reference only. Errors: `InvalidError` for a name that is no map of the
layer (at the call for a picture layer, at `add` for a reference), a map that is a hole when the layer renders (naming the
layer and the map), a `str` mask on a layer that has no picture, `use="luminance"` with a `str` map, `use` another word,
`feather` below 0 or not finite.

### Looks

`look(recipe)` runs an image look on the layer. `recipe` is an image `Recipe` (anything with `to_dict()`), the dict it
writes, or a `Path` to a recipe file; a `str` is `TypeError` (pass `Path(...)`). Looks apply in call order. See "Data in".

- **Space.** A look works on the layer's own fitted raster, not on the canvas: a vignette follows the layer's box, a
  centred warp its centre. A look on a group (`Layer.scene`) sees the whole group. This keeps a layer's result independent of
  the others, so it caches by itself.
- **Design pixels.** A `px` value of a look is a pixel of the scene's canvas as the layer is fitted, before `place`: 2 px grain
  on a small inset is as coarse as on the photo behind it, whatever the inset's size. `render(size)` multiplies it, as
  image's `render(size=)` does. A layer that `place` then scales takes its grain and its halo with it. (C1: image's own
  convention, a pixel of the layer's drawn picture whose long side is 480: see "What is built".)
- **Light that spills.** A look sees the layer's colour over black and the layer's box grows by the look's footprint (the
  room). A layer that is not light keeps its source's alpha, so the halo a bloom adds outside the shape is cut. A light layer
  shows it, and may extend past its box. A glow is a reference made light:
  `Layer.of("logo").look(bloom).as_light().blend("screen")`. Darkness outside a shape (a drop shadow, an emboss shadow) is
  a layer of its own: a fill masked by the shape, `composed_world.py`, `chrome_type.py`.
- **Size and frame.** Scene owns the layer's size and fit: a recipe that sets `size` or `frame` is `InvalidError`. Regions
  are fine.
- **Maps.** A look's regions see the picture's `maps`. A look that needs a map (`subject`, `depth`, a `given` region) that
  the layer lacks, by name or as a hole, is `InvalidError` at render naming the layer, the step and the map. Scene runs no
  model and passes image no finder: a subject or depth map is found by the caller with image and handed in, so the error
  and every look are testable without a model.

## Data in

| Layer | Takes | Scene needs of it |
|---|---|---|
| `look(recipe)` | an image `Recipe`; its `to_dict()`; a recipe file `Path` (read when called) | steps that are plain data, and no `size` or `frame`. A `Picture` or `Lut` parameter (`to_dict()` leaves the object) is `InvalidError` naming the step; give a path in a recipe file |
| `Layer.drawing(source)` | a vector `Drawing`; its `to_json()` text or dict; a drawing file `Path` (read when called) | a frame (`size`). A bare `Path` of the vector kind has no `to_json`: `TypeError`, wrap it in `btvec.Drawing(path)` |
| `Layer.picture(source)` | an array or a `Path` | in JSON a path with a digest |

A `pathlib.Path` is a file; a `str` is document text (a drawing's JSON) or a name (a bundled font). A look or drawing is checked
when it is given, by the crate scene links, so a typo fails at the call naming the layer and the step. A file given is read
once, there, and embedded: a saved scene is self-contained. A picture, a font file or a map file is not embedded; it is read
at `render`.

**Version skew.** A saved vector document carries `"version": 1`; an image recipe carries a `format` in a recipe file
(`Recipe.to_dict()` writes none, so scene stamps the format of the crate it links). A reader refuses what the linked crate
does not read, naming where it is, the version found, the versions read and the sibling crate's version:

```
FormatError: /layers/2/drawing: vector document version <found>; this scene reads vector versions <read> (bullterrier-vector <version>)
FormatError: /layers/0/looks/0: image recipe format <found>; this scene reads image formats <read> (bullterrier-image <version>)
```

A scene itself carries `"version": 1`; its format changes only by a new number, and a release that does so keeps reading the
old ones.

## Values

`scene.parameters()` returns `dict[str, Parameter]`, layer by layer: the layer's fields, its source's, its looks', its masks'.
The key is the address. `Parameter` is a frozen dataclass of the parameter format's fields with `address` standing for
`name` (a dotted address is not a valid `name`): `address`, `kind`, `meaning`, `default`, `unit`, `min`, `max`, `cyclic`,
`choices`, and `value`, what the scene holds now. Each range holds only valid values of that number alone, so a slider built
from it cannot ask for an error by itself. A rule across two numbers of an embedded look or drawing (image's `Band.low` at
most `high`) is the sibling's: `with_values` refuses it with the sibling's message.

| Address | Kind | Unit, range | Default |
|---|---|---|---|
| `<layer>.opacity` | float | fraction, 0..1 | 1 |
| `<layer>.blend` | choice | the modes above | `normal` |
| `<layer>.light` | bool | | false |
| `<layer>.dx`, `.dy` | float | px, -1e6..1e6 | 0 |
| `<layer>.scale` | float | ratio, 0.001..1000 | 1 |
| `<layer>.aspect` | float | ratio, 0.001..1000 (y only) | 1 |
| `<layer>.rotate` | float | degrees, cyclic | 0 |
| `<layer>.skew_x`, `.skew_y` | float | degrees, -89..89 | 0 |
| `<layer>.flip_x`, `.flip_y` | bool | | false |
| `<layer>.picture` | picture | | none |
| `<layer>.map.<name>` | picture | a map: float32 (h, w) 0..1, or a grey image | none |
| `<layer>.text` | text | at least 1 character | the content |
| `<layer>.color` | color | of a text or fill layer | the colour |
| `<layer>.look.<image address>` | as image | as image | as the recipe |
| `<layer>.look#2.<image address>` | | | the second look |
| `<layer>.drawing.<param>` | float | as the vector `Param` | its default |
| `<layer>.scene.<address>` | | | an address of the inner scene |
| `<layer>.mask.<field>` | | | a field of the mask layer; `mask#2` for the second |

The first segment is the layer's name; the next is a field, `look`, `drawing`, `scene` or `mask`, and the rest is the sibling's
own address, verbatim (`bloom#2.radius`, `each.looks[0].gain`, `region.near.low`). A reference has the fields of a layer and
its looks, and no source parameter. Examples: `title.dx`, `photo.picture`, `windows.look.swell.amount`,
`logo_glow.look.halo.amount`, `logo.drawing.ring_on`, `badge_b.scene.label.text`, `shadow.mask.dx`.

Image's Python `Parameter` says `doc`, `minimum`, `maximum` and vector writes version-1 units; scene writes the format's
spelling for both (`meaning`, `min`, `max`; `deg` is `degrees`, `cycle` is `cycles`, `""` is `None`).

`with_values` rules:

- A value is a number (a `float`, an `int` or a numpy number; a whole-number parameter takes `3.0`), a colour (`"#rrggbb"` or
  `"#rrggbbaa"`), a string, a choice's name, a bool, or for a picture or a map a `Path`, an array or `None` (a hole again).
- A value outside its range is refused, never clamped, naming the address, the range and the value; a cyclic one wraps.
- An address in an embedded look or drawing changes that layer's copy, checked afresh by the sibling's own rules.

## Output

`render(size)`: `size` is the long side in output pixels (image's convention), a whole number from 1 to 65,000 with at most
2^27 pixels in all (a float32 RGBA array that size is 2 GiB; larger prints wait for bands and O1); another is `InvalidError`
naming `size`. The canvas keeps its aspect: a canvas of 720 by 1280 at `size=1280` is 720 by 1280 pixels; each design pixel
is `size / max(width, height)` output pixels along the long axis; along the short axis the side is rounded up to whole
pixels and the picture stretched onto them, so a design pixel there is `rows / height` output pixels, which differs from
the long axis's by less than a pixel over the canvas, and visibly on a thin one ("What is built", Boxes).

The result is a new float32 array (height, width, 4), 0..1, rows top to bottom, straight alpha, transparent where nothing is
drawn. Colours are sRGB values. A smaller size shows the same picture in less detail. `render` reads the files the scene names
(pictures, fonts, maps) and writes none. A hole is `InvalidError` naming `<layer>.picture`; a file that is missing, unreadable
or whose digest differs is `ReadError`; an error from an embedded look or drawing at draw time is re-raised as `InvalidError`
naming the layer, with the sibling's message.

**Cache.** Scenes derived from one another by `add` and `with_values` share a bounded in-process cache of what each layer
drew. The result depends only on the scene, its values, the size and the content of the files; caching skips work and does
not change a result, with one limit: a file is keyed in the cache by its path, size and modification time, so a change that
keeps all three is not seen. A saved scene's digest (computed at `to_json`, so a path set by `with_values` is digested then)
is checked when it is read at `render`, which does see such a change. Same scene, values and size give the same array on the
same machine and library versions. The cache is observable through a plugin layer that counts its `draw` calls
([cache_probe.py](examples/cache_probe.py), C7).

PNG is image's: `imageio.save(scene.render(1080), "poster.png")`. Scene has no `render_png`.

## Saved scene

`to_json()` writes one self-contained document; `Scene.from_json(text, base=folder)` reads it back and the two give the same
pictures. Keys, in this order:

```json
{
  "version": 1,
  "canvas": {"width": 1280, "height": 720},
  "layers": [
    {"name": "photo", "picture": {"path": "photos/placeholder.jpg", "sha256": "9f2c..."},
     "fit": {"box": [0, 0, 1280, 720]}},
    {"name": "band", "fill": "#101018", "fit": {"box": [0, 480, 1280, 240], "how": "stretch"}, "opacity": 0.82},
    {"name": "title", "text": "Episode title", "font": "bricolage_grotesque", "color": "#ffffff",
     "options": {"size": 84, "width": 1080, "axes": {"wght": 800}}, "place": {"dx": 100, "dy": 590}}
  ]
}
```

A layer holds its source (`picture` with its `maps`, `drawing`, `text`, `fill`, `scene`, `of` or `kind` and `options`), `fit`, `place`,
`opacity`, `blend`, `light`, `masks` (each a layer, or `{"map": "subject"}`) and `looks` (each `{"format": <n>, "recipe":
{...}}`), a key only where it differs from its default. A drawing is the vector document inline, a scene layer the inner
document inline. A raster, a font file or a map (`"maps": {"subject": {...}}`, next to `picture`) is `{"path": ..., "sha256": ...}`, relative to the scene file's folder
when it can be (a mismatch is `ReadError` at `render`); a bundled font is its name. A path where a look or drawing belongs
(`"looks": ["looks/night.toml"]`) is read when the scene is loaded and written inline by the next `to_json()`. `FormatError`
carries the JSON Pointer of the value and what is wrong; it is also a `ValueError`.

## Errors

| Error | When | Carries |
|---|---|---|
| `BullterrierSceneError` | the base of every error scene raises for a caller's input | |
| `InvalidError` | a value or argument scene refuses, also a `ValueError`; at the call that got it, else at `render` | the argument or address, the rule and the value; for an embedded look or drawing, the layer and the sibling's message |
| `FormatError` | a document `from_json` cannot read; also a `ValueError` | the JSON Pointer; a version skew names versions as above |
| `ReadError` | a file the scene names cannot be read, or its digest differs; also an `OSError` | the layer, the path, and for a digest both digests |

A wrong type in the calling code (`Layer.drawing(path)`, `Layer.picture("a.jpg")`, `add(scene)`) is a plain `TypeError`
naming the fix. Plain numbers are checked when a call is made; a hole, a map's size and files when the scene renders. Depth
is bounded (`MAX_DEPTH`, `MAX_LAYERS`) and every walk (references, nesting) is iterative, so none needs more than a 256 KB
thread stack.

## Conventions

- **Names in the field's own terms.** `dx`, `dy`, `anchor`, `align`, `feather`, `opacity`, `color`, `box`; never `strength`,
  `factor`, `amount`, `val`. One name per concept: the pivot of `place` is `anchor`, where `fit` puts the slack is `align`,
  an offset is `dx`, `dy` (as vector's `translate`), a colour is `color`, a position inside a box is a fraction 0..1.
- **Units.** Design pixels for lengths, degrees clockwise for angles (wrapping), fractions 0..1 for opacity, anchors and
  alignments, ratios for scale. Time does not exist.
- **Arguments.** What a call cannot do without is positional (`opacity(0.5)`, `mask(by)`, `fit(box)`); every option is
  keyword-only. `place` has no required argument, so all of it is keyword-only.
- **Colours** are `"#rrggbb"` or `"#rrggbbaa"` everywhere, one spelling in all three libraries (vector refuses `"#rgb"`).
- **Paths.** A `pathlib.Path` is a file, a `str` is text or a name.
- **Alpha and colour.** Straight alpha at every boundary, premultiplied inside; blends in sRGB values.
- **A look and a drawing keep their own words.** Blend modes of a layer are CSS names (`soft-light`); inside a look image's
  stay (`soft_light`).
- **How a method is documented.** One line that states the promise in the field's terms; each argument with its unit, range and
  default; each error and when. Never how it works.

## Languages

The same names in each language's case: `snake_case` in Python and Rust.

| | Python | Rust |
|---|---|---|
| Constructors | `bts.Scene(1800, 2400)`, `bts.Layer.picture(photo)` | `Scene::new(1800, 2400)?`, `Layer::picture(Raster::rgba(w, h, pixels)?)`, `Layer::picture_file(path)`, `.map("subject", Map::new(w, h, values)?)`, `Layer::drawing(json)?`, `Layer::fill("#f2ece0")?`, `Layer::scene(inner)`, `Layer::of("logo")` |
| Data in | a look or drawing object by its `to_dict()` or `to_json()`; an array | a `&str` or `serde_json::Value` of the document; a scene-owned `Raster` (width, height, `f32` RGBA) and `Map`. The pinned image and vector crates' types never appear in scene's public API: they stay inside two adapter files and out of its semver |
| Options | keyword arguments | a value named after the call, built with `new` and setters: `Place::new().dx(100.0).rotate(Deg(-4.0))`, `Mask::new(by).feather(8.0).invert()`; the required argument alone converts into it: `layer.mask(by)`. Text: `Text::new("bricolage_grotesque").color("#ffffff").option("size", 84.0)`, each option plain data (`Into<serde_json::Value>`) |
| Modes | strings: `"cover"`, `"screen"`, `"light"` | enums: `Fit::Cover`, `Blend::Screen`, `Use::Luminance` |
| Values | `scene.with_values({"title.text": "Pilot"})` | `scene.with_values(&Values::new().set("title.text", "Pilot"))?` |
| Output | a numpy array | a `Raster` of the output size; a caller encodes it (PNG is O1) |
| Errors | exceptions | `Result<_, bullterrier_scene::Error>`. One rule: a call that can be refused returns `Result` (`add`, `named`, `fit`, `place`, `perspective`, `opacity`, `look`, `mask`, `with_values`, `render`, the constructors that parse); a call that cannot (`blend`, `as_light`, `Layer::of`, `Layer::scene`) returns the value. A scene never defers an error. `Error` is `#[non_exhaustive]`: `Invalid { address, message }`, `Format { pointer, message }`, `Read { path, message }` |
| Plugin kinds | an entry point; `bts.register_layer` | a `Source` trait with the one question "draw this rectangle at this scale", registered in a `Registry` |

[cover_template.rs](examples/cover_template.rs) is the cover template in Rust.

## What the demos asked

Every row of "What the demos and the experiment asked of scene" in [roadmap.md](roadmap.md), and where an example covers it.

| Need | Covered by |
|---|---|
| Move, scale and stand a cut-out subject on a floor | `fit(..., align=(0.5, 1.0))`: [composed_world.py](examples/composed_world.py), `dog` |
| An effect on one layer only: the logo's glow must not light the windows; the sun glows, the dog does not | a reference with a look, made light: [mill.py](examples/mill.py) `logo_glow`, [composed_world.py](examples/composed_world.py) `sun_glow`, [wow_poster.py](examples/wow_poster.py) `rings_glow` |
| Contact shadow and floor reflection of a cut-out | references to the dog, placed and masked: [composed_world.py](examples/composed_world.py) `reflection`, `shadow` |
| Reframing to the delivery shape without an outside crop | `fit(box, "cover", align=...)`: [mill.py](examples/mill.py) `night` |
| One colour typed once, for vector, image and the caller's code | in part: a scene's own colour is one parameter ([cover_template.py](examples/cover_template.py) `band.color`); inside a look or a drawing it waits for a colour reference in image and vector (Needs from others) |
| A region of a layer as the target of an effect: the sign, each window | a region of an image look as a named map, `windows` in [mill.py](examples/mill.py) (`region`); a drawing as a mask, `sign_glow` (`sign_box`) |
| An effect's space: the layer's own picture, or the canvas | the layer's own picture ("Looks", "Space"): [wow_poster.py](examples/wow_poster.py) `inset`, [chrome_type.py](examples/chrome_type.py) `type` |
| A static layer renders once | [cache_probe.py](examples/cache_probe.py); [frame_sequence.py](examples/frame_sequence.py) changes only the headline |
| Values per frame arrive as `scene.with_values(...)` | [frame_sequence.py](examples/frame_sequence.py), [mill.py](examples/mill.py) `frame_values` |
| What scene does not take on: facade detection, prompted segmentation, footage mattes, shot cuts, a timeline | "What this API deliberately does not have"; the matte is the caller's: `subject_matte` in [mill.py](examples/mill.py) |

## What this API deliberately does not have

- **No class per kind.** One `Layer`, one mode string for blends, one `mask`, one `look`; no group type (a scene is a group), no
  mask type (a layer is a mask), no gradient (vector's).
- **No mutation.** Every call returns a new value.
- **No callbacks in a scene.** A scene saves as JSON and renders where no caller's code runs. A plugin kind is named, with
  plain options.
- **No time.** No clock, easing or signal: a parameter is a state; the caller sets it and renders again.
- **One way to set values.** `with_values`; footage is a picture parameter.
- **No implicit IO.** `render` reads the files the scene names and writes none; `to_json` writes nothing, the caller saves it.
- **No models.** A subject or depth map is found by the caller with image.
- **No global state.** No current scene, canvas, font or random generator. The cache is keyed by content, not held by the caller.
- **No configuration objects.** Options are keyword arguments of the call they shape.
- **No reorder or remove.** A scene is rebuilt from its layers (`Scene(w, h).add(*layers)`).

## Decisions

Each has its reason in [design-decisions.md](design-decisions.md): immutable values; looks and drawings enter as data; one way
to set values; addresses are the layer name and the sibling's address verbatim; a reference layer instead of a copy; alpha is
an explicit choice; a mask is a layer or a named map; placement is fixed fields with valid ranges; one hand-written warp, and a layer drawn at the scale it will have; text options are vector's;
one shared in-process cache; a group is a scene; one picture layer with a hole; gradients are vector's; where the entire work
ends.

## Needs from others

| From | Need | By |
|---|---|---|
| image | `Recipe.to_dict()` writes `format`, a public list of the formats it reads, and a public crate version for the messages. **Not met at the pin** (its `to_dict()` writes none, the list is private): C1 keeps a copy of the list and omits the version | C6 |
| vector | a public list of the document versions it reads and a public crate version, for the version-skew message | C6 |
| image | a look refused for its picture's size says "`frame` makes a picture of ..." when no frame is set, and a missing map's message names image's Rust API (`Photo::with_map`); scene passes both through | C3 |
| image | a public call that returns a region of a look as a map (today `mill.py` matches a `trace` stage by its label) | before C3 |
| image | alpha carried through looks (a drop shadow outside a shape) | later |
| image | a `Parameter` spelled as the format (`meaning`, `min`, `max`), and `Picture` and `Lut` parameters that `to_dict()` can write as data | later |
| image, vector | a colour reference, so one colour is typed once across a scene, its looks and its drawings | C6 |
| vector | a distance-field relief (V15), a later refinement of chrome type; not needed by `chrome_type.py` | later |
| design repo | parameter format v4: an `address` where `name` stands (its schema pattern `^[A-Za-z][A-Za-z0-9_]*$` refuses a dotted address) | before C6 |

## Open questions

1. **One colour across scene, looks and drawings.** Image and vector validate colours as hex when they build, so a scene cannot
   push a name into them. See "Needs from others". Until then a colour is a Python constant.
2. **Dark light past a shape.** A light layer makes alpha from brightness, so a dark halo (image's `drop_shadow`) is
   invisible; a shadow is a fill masked by the shape. Lifts when image carries alpha through its effects.
3. **`MAX_DEPTH` (16) is a placeholder** until scenes can be layers (C4). `MAX_LAYERS` and 2^27 pixels are measured ("What is built").
4. **The cache's bound** (entries or bytes) is set when C7 measures a 15-layer A3 poster.
