# The code API

Status: A1, designed from the example programs in [examples/](examples/). Nothing is built yet: every signature below is
what batches C1 to C7 make real, and each example is the acceptance material of the batch that makes it run. Reasons:
[design-decisions.md](design-decisions.md). The JSON form of a scene is "Saved scene" below.

Signatures are Python's; "Languages" gives the Rust form. The library is imported under the alias `bts` in every language.

```python
import bullterrier_scene as bts
```

## Examples

Each program is a `build(...)` function that returns a `Scene` from its inputs (a photo array, a matte, a path), and a
`__main__` block that loads real files and renders. A test calls `build` with synthetic inputs and needs no model.

| Program | Shows |
|---|---|
| [poster_two_ink.py](examples/poster_two_ink.py) | M1: a photo in two inks clipped into a polygon, type behind a cut-out subject, flat shapes, grain, three sizes |
| [cover_template.py](examples/cover_template.py), [.rs](examples/cover_template.rs) | C6: one scene, a photo, a title and a colour set by `with_values`; saved as JSON and read back; six covers |
| [frame_sequence.py](examples/frame_sequence.py) | C7: a headline sliding in over 24 frames, one value set per frame |
| [cache_probe.py](examples/cache_probe.py) | C7: a layer that did not change is not drawn again, observed through a counting plugin layer |
| [composed_world.py](examples/composed_world.py) | M2: a cut-out dog on a grid floor under a striped sun; shadow and reflection by reference, a glow on the sun only |
| [mill.py](examples/mill.py) | M3: a night-graded photo, windows and sign lit as layers, a logo with its own glow, a 9:16 crop by `fit`, every moving value per frame |
| [badge_scene_in_scene.py](examples/badge_scene_in_scene.py) | a scene used as a layer, twice, with different words |
| [wow_poster.py](examples/wow_poster.py) | the final poster: layered type, cut-out, neon, halftone, glow on chosen layers, mixed blends |

## Five ideas

**`Scene`** is a canvas (width and height in design pixels) and a stack of layers, bottom first. It is an immutable value:
every call returns a new scene, the old one is unchanged and can be used again. It renders at any size: `scene.render(1080)`.

**`Layer`** is an immutable value too: a source (a picture, a drawing, type, a flat colour, another scene, or another layer
of the same scene), how it sits (`fit`, `place`, `perspective`), how it joins what is below (`opacity`, `blend`, `alpha`),
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
| `to_json` | `to_json()` | the scene as JSON text, self-contained ("Saved scene") | `InvalidError`: a picture that is an array (it has no file). `ReadError`: a file whose digest is to be written cannot be read |
| `Scene.from_json` | `Scene.from_json(text, *, base=None)` | the scene back; a relative path in it resolves against folder `base` (a `Path`; default: the working folder). Reads no picture, font or map: those are read at `render` | `FormatError` |

Scenes and layers are equal when they hold the same data (a picture by its content digest, a drawing and a look by their
serialised form). Equal scenes with equal values render equal pictures.

## Layer

### How a layer is drawn

One order, whatever the order of the calls; the calls matter only among looks and among masks of one kind. Each stage names
the space it works in: the layer's space is its own fitted picture, the canvas is the scene's.

1. **Source.** The layer's picture is drawn at its box (layer space).
2. **Fit.** `fit` scales and crops it into its box; without `fit` it stays one design pixel per source pixel.
3. **Looks,** in call order, on the fitted picture (layer space). A look that has a centre or an edge (a vignette, a warp)
   takes it from the fitted box. Looks see the layer's colour over black.
4. **Own-map masks** (`mask("name")`): layer space; the map is cut and fitted with the picture.
5. **Place.** Flip, aspect, scale, skew and rotate about `anchor`, then `dx`, `dy` (or `perspective`): the layer goes onto the canvas.
6. **Layer masks** (`mask(Layer)`): canvas space, multiplied.
7. **Alpha choice** (`alpha`).
8. **Opacity.**
9. **Blend** over what is below.

A `Layer.of` runs stages 1 to 6 of the layer it names, with that layer's values, and adds its own at each stage ("Reference").

### Sources

| Source | Signature | Promise |
|---|---|---|
| picture | `Layer.picture(source=None, *, maps=None)` | `source`: an array (height, width, 3 or 4; `uint8`, `uint16` or floats 0..1; 4 channels are straight alpha), a `pathlib.Path` (read as `imageio.load_picture` reads: upright, sRGB, alpha kept), or `None`: a hole the caller fills with `with_values` before `render`. `maps`: the picture's named maps, each a float32 (height, width) array 0..1 of the picture's size (`{"subject": matte}`). A look sees the maps its recipe uses (`subject`, `depth`, a `given` region); the others serve `mask("name")`. A `str` is `TypeError` (pass `Path(...)`). Its box is its pixels, one design pixel each, at the canvas origin. An array from `imageio.load` has no alpha (image puts it over white): give a transparent PNG as a `Path` or `imageio.load_picture` |
| drawing | `Layer.drawing(source, *, inputs=None)` | `source`: a vector `Drawing` (anything with `to_json()`), its JSON text (a `str`) or dict, or a `Path` to a drawing file. Its box is the drawing's frame (`size`) at the canvas origin: a drawing without a frame is refused. `inputs`: the bytes or `Path` of each font or map file the drawing names, by name (vector's `Drawing.from_json(inputs=)`); a bundled font needs none |
| text | `Layer.text(content, *, font, color="#000000", **text_options)` | scene owns `content`, `font` and `color`. `font`: a bundled font's name (a `str`) or a `Path` to a font file. `text_options` are `btvec.text`'s (`size`, `axes`, `tracking`, `leading`, `align`, `width`, `fit`, `features`...) as plain data, validated by the linked vector crate at the call; scene restates none of vector's defaults. `anchor` and `inside` are refused, and so are expressions and Params: scene places the text, and per-letter numbers are a `Layer.drawing`. Its origin is the text's anchor, the left end, middle or right end of the first baseline by `align`: `place(dx=100, dy=420)` puts it there. Its box is the glyphs' bounds |
| fill | `Layer.fill(color)` | a flat `"#rrggbb"` or `"#rrggbbaa"` over the canvas box; `fit` makes it a band, a mask makes it a shape. Gradients are vector's: a rectangle filled with `btvec.linear_gradient` |
| scene | `Layer.scene(inner)` | another scene as a group: its canvas is its box; it blends, fades, masks and takes a look as one picture. `inner`'s parameters are addressed under the layer: `badge.scene.label.text` |
| reference | `Layer.of(name)` | see "Reference" |
| plugin | `Layer.make(kind, **options)` | a layer of a registered kind, saved by `kind` and `options`. A kind is registered by name in the entry-point group `bullterrier_scene.layers` and modifies nothing in the package; it is asked for one rectangle of the canvas, `(x, y, width, height)` in design pixels, at a scale in output pixels per design pixel, and returns its picture. `bts.register_layer(kind, draw)` registers one in process (a test's way). Arrives with C7: the cache test counts the `draw` calls of such a kind |

Things that must move together (a logo and its glow, a dog and its shadow) are one layer and its references. When a group
of layers must move together, make them one inner scene and use it with `Layer.scene`.

### Reference

`Layer.of(name)` draws the layer called `name` of the same scene, again. It runs that layer's stages 1 to 6 with that layer's
own values, so its source and placement parameters stay at the named layer's address: `logo.drawing.ring_on` is set once and
the logo's glow follows. At each stage it adds its own: its looks after the named layer's, its own-map masks, a `place` applied
after the named layer's (about its own `anchor`, over the placed box), its layer masks. Then it has its own alpha choice,
opacity and blend; the named layer's are not used. It has no source, no `fit` and no `perspective` (`InvalidError`).

```python
logo_glow = bts.Layer.of("logo").named("logo_glow").look(halo).alpha("light").blend("screen")
shadow = bts.Layer.fill("#000000").mask(
    bts.Layer.of("dog").place(scale=0.12, aspect=8.3, anchor=(0.5, 1.0)), feather=12
)
```

A reference names a layer of its own scene that is already added or is in the same `add` call; it may name another reference.
Its own parameters are the fields of any layer (`opacity`, `blend`, `alpha`, the placement fields) and its looks: `logo_glow.look.halo.amount`.

### Placement

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `named` | `named(name)` | the layer called `name`: letters, digits and `_`, starting with a letter, unique in its scene. A layer without a name is `layer1`, `layer2`... by its place in the scene, which shifts when a layer is inserted: name every layer you set values on or refer to. `layer<n>` is refused as a name for any other place | `InvalidError` |
| `fit` | `fit(box, how="cover", *, align=(0.5, 0.5))` | the layer's box fitted into `box` = `(x, y, width, height)`, design pixels. `"cover"`: scaled, aspect kept, to fill `box`, the overflow cut off at `box`'s edge; `"contain"`: scaled to lie wholly inside; `"stretch"`: `box` exactly, aspect not kept. `align` (fractions 0..1 of the layer's box) says which part stays when cover cuts and where a contained layer sits in the slack. Resolved at render from the picture's real size, so a hole's picture of any size fills the same box; cover is the delivery crop. Not a parameter | `InvalidError`: `how` another word, `box` with a size not above 0, `align` outside 0..1 |
| `place` | `place(*, dx=None, dy=None, scale=None, aspect=None, rotate=None, skew=None, flip=None, anchor=None)` | sets the fields it names and keeps the others. After `fit`, the layer is flipped by `flip` (`(x, y)` booleans), stretched by `aspect` (x only) and scaled by `scale` (uniform), skewed by `skew` (`(x, y)` degrees) and rotated by `rotate` (degrees clockwise, any size, wrapping), all about `anchor` (fractions 0..1 of the fitted box, default the centre), then moved by `dx`, `dy` design pixels. Defaults: no change | `InvalidError`: `scale` or `aspect` outside 0.001..1000, a skew outside -89..89, a non-finite number |
| `perspective` | `perspective(corners)` | the four corners of the fitted box land on `corners` (top left, top right, bottom right, bottom left; design pixels), replacing `place`'s transform (the last of the two called wins). Not a parameter | `InvalidError`: corners that cross or are collinear |

`place`'s fields are parameters: `dx`, `dy` (px), `scale`, `aspect` (ratio), `rotate` (degrees, cyclic), `skew_x`, `skew_y`
(degrees), `flip_x`, `flip_y` (bool). `scale` is the zoom of an animation; a flat shadow is `scale=0.12, aspect=8.3`. A layer
is resampled once, from its source to the output, so a draft and a print differ in detail only.

### Joining the layers below

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `opacity` | `opacity(alpha)` | sets the layer's opacity to `alpha`, 0..1 (default 1). A group fades as one | `InvalidError`: outside 0..1 |
| `blend` | `blend(mode)` | how the layer joins what is below, the W3C compositing modes by their CSS names: `"normal"` (default), `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`, `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`. Blended in sRGB values, as vector and a browser do. A look's own `blend_mode` (image's spelling: `soft_light`) is inside the look and stays image's | `InvalidError`: another word |
| `alpha` | `alpha(choice)` | where the layer's alpha comes from. `"source"` (default): the alpha of its source, after its masks; a halo or shadow a look adds outside the shape is lost. `"light"`: the layer is its look's colour over black and its alpha is the brightness (the largest of R, G, B) of that colour, so a glow shows beyond the shape; black adds nothing, white is opaque white. Masks still scale it | `InvalidError`: another word |

### Masks

`mask(by, *, use="alpha", feather=0, invert=False)` multiplies the layer's alpha by a mask. Calling it again multiplies
another mask in.

| Argument | Meaning |
|---|---|
| `by` | a `Layer`: drawn on the canvas as its own place says (stage 6), read as `use` says; it may be `Layer.of("subject")`, which follows `subject`'s parameters. Or a `str` naming one of this layer's own `maps`: a map of its picture, cut and fitted with it (stage 4). A reference reads the `maps` of the layer it names |
| `use` | `"alpha"` or `"luminance"` (0.2126 R + 0.7152 G + 0.0722 B of the sRGB values, times the alpha: white keeps, black hides), for a `Layer`; a map is its own values |
| `feather` | the standard deviation, in design pixels, of a Gaussian blur of the mask's edge; 0 is hard |
| `invert` | 1 minus the mask (after the feather): type that passes behind a subject is masked by the inverted subject |

A mask layer is owned by the layer it masks and is outside the scene's names: it is addressed `<layer>.mask.<field>`, the
second `mask#2`. A mask layer's alpha includes its own masks, not its looks. A `Layer.of` mask has the fields of a
reference only. Errors: `InvalidError` for a name that is no map of the layer (at the call for a picture layer, at `add` for
a reference), a map that is not 2-D, not 0..1 or of another size than its picture (at render, naming both sizes), a
`str` mask on a layer that has no picture, `use="luminance"` with a `str` map, `use` another word, `feather` below 0 or not
finite.

### Looks

`look(recipe)` runs an image look on the layer. `recipe` is an image `Recipe` (anything with `to_dict()`), the dict it
writes, or a `Path` to a recipe file; a `str` is `TypeError` (pass `Path(...)`). Looks apply in call order. See "Data in".

- **Space.** A look works on the layer's own fitted picture, not on the canvas: a vignette follows the layer's box, a
  centred warp its centre. A look on a group (`Layer.scene`) sees the whole group. This keeps a layer's result independent of
  the others, so it caches by itself.
- **Design pixels.** A `px` value of a look is a pixel of the scene's canvas, whatever the layer's size: 2 px grain on a
  small inset is as coarse as on the photo behind it. `render(size)` multiplies it, as image's `render(size=)` does.
- **Light that spills.** A look sees the layer's colour over black. With `alpha("source")` the layer keeps its source's
  alpha, so light a look adds outside the shape (a bloom's halo) is cut. With `alpha("light")` it shows. A glow is a
  reference with `alpha("light")`: `Layer.of("logo").look(bloom).alpha("light").blend("screen")`. Darkness outside a shape
  (a drop shadow) is a layer of its own: a fill masked by the shape, `composed_world.py`.
- **Size and frame.** Scene owns the layer's size and fit: a recipe that sets `size` or `frame` is `InvalidError`. Regions
  are fine.
- **Maps.** A look's regions see the picture's `maps`. Scene runs no model: a subject or depth map is found by the caller
  with image and handed in.

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
`choices`, and `value`, what the scene holds now. Every range holds only valid values, so a slider built from it cannot ask
for an error.

| Address | Kind | Unit, range | Default |
|---|---|---|---|
| `<layer>.opacity` | float | fraction, 0..1 | 1 |
| `<layer>.blend` | choice | the modes above | `normal` |
| `<layer>.alpha` | choice | `source`, `light` | `source` |
| `<layer>.dx`, `.dy` | float | px, -1e6..1e6 | 0 |
| `<layer>.scale` | float | ratio, 0.001..1000 | 1 |
| `<layer>.aspect` | float | ratio, 0.001..1000 (x only) | 1 |
| `<layer>.rotate` | float | degrees, cyclic | 0 |
| `<layer>.skew_x`, `.skew_y` | float | degrees, -89..89 | 0 |
| `<layer>.flip_x`, `.flip_y` | bool | | false |
| `<layer>.picture` | picture | | none |
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
  `"#rrggbbaa"`), a string, a choice's name, a bool, or for a picture a `Path`, an array or `None` (a hole again).
- A value outside its range is refused, never clamped, naming the address, the range and the value; a cyclic one wraps.
- An address in an embedded look or drawing changes that layer's copy, checked afresh by the sibling's own rules.

## Output

`render(size)`: `size` is the long side in output pixels (image's convention), a whole number from 1 to 65,000 with at most
2^27 pixels in all (a float32 RGBA array that size is 2 GiB; larger prints wait for bands and O1); another is `InvalidError`
naming `size`. The canvas keeps its aspect: a canvas of 720 by 1280 at `size=1280` is 720 by 1280 pixels; each design pixel
is `size / max(width, height)` output pixels.

The result is a new float32 array (height, width, 4), 0..1, rows top to bottom, straight alpha, transparent where nothing is
drawn. Colours are sRGB values. A smaller size shows the same picture in less detail. `render` reads the files the scene names
(pictures, fonts, maps) and writes none. A hole is `InvalidError` naming `<layer>.picture`; a file that is missing, unreadable
or whose digest differs is `ReadError`; an error from an embedded look or drawing at draw time is re-raised as `InvalidError`
naming the layer, with the sibling's message.

**Cache.** Scenes derived from one another by `add` and `with_values` share a bounded in-process cache of what each layer
drew. The result depends only on the scene, its values, the size and the content of the files; caching skips work and never
changes a result. A file is keyed in the cache by its path, size and modification time; its digest is computed when a scene is
saved (`to_json`, so a path set by `with_values` is digested then) and checked when a saved scene is read at `render`. Same
scene, values and size give the same array on the same machine and library versions. The cache is observable through a plugin
layer that counts its `draw` calls ([cache_probe.py](examples/cache_probe.py), C7).

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
     "fit": {"box": [0, 0, 1280, 720], "how": "cover", "align": [0.5, 0.5]}},
    {"name": "band", "fill": "#101018", "fit": {"box": [0, 480, 1280, 240], "how": "stretch"}, "opacity": 0.82},
    {"name": "title", "text": "Episode title", "font": "bricolage_grotesque", "color": "#ffffff",
     "options": {"size": 84, "width": 1080, "axes": {"wght": 800}}, "place": {"dx": 100, "dy": 590}}
  ]
}
```

A layer holds its source (`picture`, `drawing`, `text`, `fill`, `scene`, `of` or `kind` and `options`), `fit`, `place`,
`opacity`, `blend`, `alpha`, `masks` (each a layer, or `{"map": "subject"}`) and `looks` (each `{"format": <n>, "recipe":
{...}}`), a key only where it differs from its default. A drawing is the vector document inline, a scene layer the inner
document inline. A raster, a font file or a map file is `{"path": ..., "sha256": ...}`, relative to the scene file's folder
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
| Constructors | `bts.Scene(1800, 2400)`, `bts.Layer.picture(photo)` | `Scene::new(1800, 2400)?`, `Layer::picture(Raster::rgba(w, h, pixels)?)`, `Layer::picture_file(path)`, `Layer::drawing(json)?`, `Layer::fill("#f2ece0")?`, `Layer::scene(inner)`, `Layer::of("logo")` |
| Data in | a look or drawing object by its `to_dict()` or `to_json()`; an array | a `&str` or `serde_json::Value` of the document; a scene-owned `Raster` (width, height, `f32` RGBA) and `Map`. The pinned image and vector crates' types never appear in scene's public API: they stay inside two adapter files and out of its semver |
| Options | keyword arguments | a value named after the call, built with `new` and setters: `Place::new().dx(100.0).rotate(Deg(-4.0))`, `Mask::new(by).feather(8.0).invert()`; the required argument alone converts into it: `layer.mask(by)`. Text: `Text::new("bricolage_grotesque").color("#ffffff").option("size", 84.0)`, each option plain data (`Into<serde_json::Value>`) |
| Modes | strings: `"cover"`, `"screen"`, `"light"` | enums: `Fit::Cover`, `Blend::Screen`, `Alpha::Light`, `Use::Luminance` |
| Values | `scene.with_values({"title.text": "Pilot"})` | `scene.with_values(&Values::new().set("title.text", "Pilot"))?` |
| Output | a numpy array | a `Raster` of the output size; a caller encodes it (PNG is O1) |
| Errors | exceptions | `Result<_, bullterrier_scene::Error>`. One rule: a call that can be refused returns `Result` (`add`, `named`, `fit`, `place`, `perspective`, `opacity`, `look`, `mask`, `with_values`, `render`, the constructors that parse); a call that cannot (`blend`, `alpha`, `Layer::of`, `Layer::scene`) returns the value. A scene never defers an error. `Error` is `#[non_exhaustive]`: `Invalid { address, message }`, `Format { pointer, message }`, `Read { path, message }` |
| Plugin kinds | an entry point; `bts.register_layer` | a `Source` trait with the one question "draw this rectangle at this scale", registered in a `Registry` |

[cover_template.rs](examples/cover_template.rs) is the cover template in Rust.

## What the demos asked

Every row of "What the demos and the experiment asked of scene" in [roadmap.md](roadmap.md), and where an example covers it.

| Need | Covered by |
|---|---|
| Move, scale and stand a cut-out subject on a floor | `fit(..., align=(0.5, 1.0))`: [composed_world.py](examples/composed_world.py), `dog` |
| An effect on one layer only: the logo's glow must not light the windows; the sun glows, the dog does not | a reference with a look and `alpha("light")`: [mill.py](examples/mill.py) `glow`, [composed_world.py](examples/composed_world.py) `sun_glow`, [wow_poster.py](examples/wow_poster.py) `rings_glow` |
| Contact shadow and floor reflection of a cut-out | references to the dog, placed and masked: [composed_world.py](examples/composed_world.py) `reflection`, `shadow` |
| Reframing to the delivery shape without an outside crop | `fit(box, "cover", align=...)`: [mill.py](examples/mill.py) `crop` |
| One colour typed once, for vector, image and the caller's code | in part: a scene's own colour is one parameter ([cover_template.py](examples/cover_template.py) `band.color`); inside a look or a drawing it waits for a colour reference in image and vector (Needs from others) |
| A region of a layer as the target of an effect: the sign, each window | a region of an image look as a named map, `windows` in [mill.py](examples/mill.py) (`region`); a drawing as a mask, `sign_glow` (`sign_box`) |
| An effect's space: the layer's own picture, or the canvas | the layer's own picture ("Looks", "Space"): [wow_poster.py](examples/wow_poster.py) `inset` |
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
an explicit choice; a mask is a layer or a named map; placement is fixed fields with valid ranges; text options are vector's;
one shared in-process cache; a group is a scene; one picture layer with a hole; gradients are vector's; where the entire work
ends.

## Needs from others

| From | Need | By |
|---|---|---|
| image | `Recipe.to_dict()` writes `format`, and a list of the formats it reads | before C1 |
| image | a public call that returns a region of a look as a map (today `mill.py` matches a `trace` stage by its label) | before C3 |
| image | alpha carried through looks (a drop shadow outside a shape) | later |
| image | a `Parameter` spelled as the format (`meaning`, `min`, `max`), and `Picture` and `Lut` parameters that `to_dict()` can write as data | later |
| image, vector | a colour reference, so one colour is typed once across a scene, its looks and its drawings | C6 |
| design repo | parameter format v4: an `address` where `name` stands (its schema pattern `^[A-Za-z][A-Za-z0-9_]*$` refuses a dotted address) | before C6 |

## Open questions

1. **One colour across scene, looks and drawings.** Image and vector validate colours as hex when they build, so a scene cannot
   push a name into them. See "Needs from others". Until then a colour is a Python constant.
2. **Dark light past a shape.** `alpha("light")` makes alpha from brightness, so a dark halo (image's `drop_shadow`) is
   invisible; a shadow is a fill masked by the shape. Lifts when image carries alpha through its effects.
3. **Limits are placeholders** (`MAX_LAYERS`, `MAX_DEPTH`, 2^27 pixels) until C1 measures them.
4. **The cache's bound** (entries or bytes) is set when C7 measures a 15-layer A3 poster.
