# The code API

Status: A1, designed from the example programs in [examples/](examples/). Nothing is built yet: every signature below is what
batches C1 to C7 make real, and each example is the acceptance material of the batch that makes it run. Review of A1: the owner
delegated it on 2026-11-13 to the advisor and one independent read-only reviewer. Reasons: [design-decisions.md](design-decisions.md). The JSON
form of a scene is "Saved scene" below.

Signatures are Python's; "Languages" gives the Rust form. The library is imported under the alias `bts` in every language.

```python
import bullterrier_scene as bts
```

## Examples

| Program | Shows |
|---|---|
| [poster_two_ink.py](examples/poster_two_ink.py) | M1: a photo in two inks clipped into a polygon, type behind a cut-out subject, flat shapes, grain, three sizes |
| [cover_template.py](examples/cover_template.py), [.rs](examples/cover_template.rs) | C6: one scene, a photo, a title and a colour set by `with_values`; saved as JSON and read back; six covers |
| [frame_sequence.py](examples/frame_sequence.py) | C7: a headline sliding in over 24 frames, one value set per frame |
| [composed_world.py](examples/composed_world.py) | M2: a cut-out dog on a grid floor under a striped sun; shadow, reflection, a glow on the sun only |
| [mill.py](examples/mill.py) | M3: a night-graded photo, windows and sign lit as layers, a logo with its own glow, a 9:16 crop by `fit`, every moving value per frame |
| [badge_scene_in_scene.py](examples/badge_scene_in_scene.py) | a scene used as a layer, twice, with different words |
| [wow_poster.py](examples/wow_poster.py) | the final poster: layered type, cut-out, neon, halftone, glow on chosen layers, mixed blends |

## Five ideas

**`Scene`** is a canvas (width and height in design pixels) and a stack of layers, bottom first. It is an immutable value:
every call returns a new scene, the old one is unchanged and can be used again. It renders at any size: `scene.render(1080)`.

**`Layer`** is an immutable value too: a source (a picture, a drawing, type, a flat colour, another scene), how it sits
(`fit`, `place`, `perspective`), how it joins what is below (`opacity`, `blend`), what hides parts of it (`mask`) and what
changes it (`look`). Every call returns a new layer. There is no class per kind of layer, blend mode or mask: a layer is
one type; a scene used as a layer is a group that blends as one.

**Looks and drawings enter as data.** A look is an image `Recipe`, a drawing a vector `Drawing`. Scene links its own copy of
both crates, so it takes what you hold by its serialised form (`to_dict()` or `to_json()`), never the Python object. A
saved scene embeds them. Scene imports neither package in Python.

**Values and parameters.** Every number, colour, text and picture of a scene has an address: `title.opacity`,
`photo.picture`, `logo.drawing.ring_on`, `windows.look.swell.amount`. `scene.parameters()` describes them in the shared
parameter format, `scene.with_values({address: value})` sets them and returns a new scene, and nothing else sets a value
(`render` takes none). A frame of an animation is `scene.with_values({...}).render(1080)`; the clock is the caller's.

**One picture out.** `scene.render(size)` is a float32 array (height, width, 4), straight alpha, what
`imageio.save` accepts. Everything else (bands, caches, resampling, premultiplication) is behind that call.

## Scene

| Member | Signature | Promise | Errors |
|---|---|---|---|
| `Scene` | `Scene(width, height)` | an empty canvas of `width` by `height` design pixels: whole numbers, 1 to 100,000. A design pixel is what a `px` value means everywhere in the scene | `InvalidError` |
| `add` | `add(*layers)` | the scene with `layers` on top of its stack, first argument lowest | `InvalidError`: a name used twice (naming both layers); more than `MAX_LAYERS` (1,000) layers; scenes nested more than `MAX_DEPTH` (16) deep. `TypeError`: a non-`Layer` (wrap a scene with `Layer.scene`) |
| `layers` | property | the layers, bottom first, a tuple | |
| `width`, `height` | properties | the canvas | |
| `parameters` | `parameters()` | every parameter, by address, in layer order ("Values") | |
| `with_values` | `with_values(values)` | the scene with `values` (a mapping from address to value) set | `InvalidError`: an address the scene lacks (the message lists the valid ones and the nearest), a value outside its range or of the wrong kind (the address, the rule and the value) |
| `render` | `render(size)` | the picture ("Output") | `InvalidError`, `ReadError` |
| `to_json` | `to_json()` | the scene as JSON text, self-contained ("Saved scene") | `InvalidError`: a picture that is an array (it has no file), a look that holds a `Picture` or `Lut` |
| `Scene.from_json` | `Scene.from_json(text, *, base=None)` | the scene back; a relative path in it resolves against folder `base` (default: the working folder). Reads no picture, font or map: those are read at `render` | `FormatError` |

Scenes and layers compare by what they would render; equal values render equal pictures.

## Layer

A layer is drawn in a fixed order, whatever the order of the calls: its source, then its looks (in the order given), then its
masks (all multiplied), placed on the canvas, then blended over what is below at its opacity. The call order matters only
among looks and among masks.

### Sources

| Source | Signature | Promise |
|---|---|---|
| picture | `Layer.picture(source=None, *, maps=None)` | `source`: an array (height, width, 3 or 4; `uint8`, `uint16` or floats 0..1; 4 channels are straight alpha), a path (read as `imageio.load_picture` reads: upright, sRGB, alpha kept), or `None`: a hole the caller fills with `with_values` before `render`. `maps`: the picture's named maps for a look's regions, each a float32 (height, width) array 0..1 of the picture's size (`{"subject": matte}`), as image's `render(maps=)`. Its box is its pixels, one design pixel each, at the canvas origin. An array from `imageio.load` has no alpha (image puts it over white): give a transparent PNG as a path or `imageio.load_picture` |
| drawing | `Layer.drawing(source, *, inputs=None)` | `source`: a vector `Drawing` (anything with `to_json()`), its JSON text or dict, or a path to a drawing file. Its box is the drawing's frame (`size`) at the canvas origin: a drawing without a frame is refused. `inputs`: the bytes or path of each font or map file the drawing names, by name (vector's `Drawing.from_json(inputs=)`); a bundled font needs none |
| text | `Layer.text(content, *, font, size=100, color="#000000", axes=None, tracking=0, leading=None, align="left", width=None)` | `content` set as `btvec.text` sets it, each argument with that meaning. `font`: a bundled font's name or a path to a font file. Its origin is the text's anchor, the left end, middle or right end of the first baseline by `align`: `place(dx=100, dy=420)` puts it there. Its box is the glyphs' bounds. For `fit`, `features`, per-letter numbers, text inside a shape: build the text with vector and use `Layer.drawing` |
| fill | `Layer.fill(color)` | a flat `"#rrggbb"` or `"#rrggbbaa"` over the canvas box; `fit` makes it a band, a mask makes it a shape. Gradients are vector's: a rectangle filled with `btvec.linear_gradient` |
| scene | `Layer.scene(inner)` | another scene as a group: its canvas is its box; it blends, fades, masks and takes a look as one picture. `inner`'s parameters are addressed under the layer: `badge.scene.label.text` |
| other | `Layer.make(kind, **options)` | a layer of a registered kind (a plugin), saved by `kind` and `options` |

A plugin kind registers by name in the entry-point group `bullterrier_scene.layers` and modifies nothing in the package. It
is an object with `parameters` (declared once, in the parameter format) and `draw(x, y, width, height, scale, **options)`,
which returns the (height, width, 4) float32 straight-alpha picture of that rectangle of the canvas (design pixels) at `scale`
output pixels per design pixel. The five kinds above are registered the same way. `options` are plain data, so a scene that
holds a plugin layer still saves as JSON.

### Placement

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `named` | `named(name)` | the layer called `name`: letters, digits and `_`, starting with a letter, unique in its scene. A layer without a name is `layer1`, `layer2`... by its place in the scene, which shifts when a layer is inserted: name every layer you set values on | `InvalidError` |
| `fit` | `fit(box, how="cover", *, anchor=(0.5, 0.5))` | the layer's box fitted into `box` = `(x, y, width, height)`, design pixels. `"cover"`: scaled, aspect kept, to fill `box`, the overflow cut off at `box`'s edge; `"contain"`: scaled to lie wholly inside; `"stretch"`: `box` exactly, aspect not kept. `anchor` (fractions 0..1 of the layer's box) says which part stays when cover cuts and where a contained layer sits in the slack. Resolved at render from the picture's real size, so a hole's picture of any size fills the same box; cover is the delivery crop. Not a parameter | `InvalidError`: `how` another word, `box` with a size not above 0, `anchor` outside 0..1 |
| `place` | `place(*, dx=None, dy=None, scale=None, rotate=None, skew=None, anchor=None)` | sets the fields it names and keeps the others. After `fit`, the layer is scaled by `scale` (a number, or `(x, y)`; negative mirrors), skewed by `skew` (`(x, y)` degrees) and rotated by `rotate` (degrees clockwise, any size, wrapping), all about `anchor` (fractions of the fitted box, default the centre), then moved by `dx`, `dy` design pixels. Defaults: no change | `InvalidError`: `scale` 0, a skew of 90 degrees or more, a non-finite number |
| `perspective` | `perspective(corners)` | the four corners of the fitted box land on `corners` (top left, top right, bottom right, bottom left; design pixels), replacing `place`'s transform (the last of the two called wins). Not a parameter | `InvalidError`: corners that cross or are collinear |

`place`'s numbers are parameters: `dx`, `dy` (px), `scale_x`, `scale_y` (ratio), `rotate` (degrees, cyclic), `skew_x`,
`skew_y` (degrees). `place(scale=2)` sets both scales. A layer is resampled once, from its source to the output, so a draft
and a print differ in detail only.

### Joining the layers below

| Call | Signature | Promise | Errors |
|---|---|---|---|
| `opacity` | `opacity(alpha)` | sets the layer's opacity to `alpha`, 0..1 (default 1). A group fades as one | `InvalidError`: outside 0..1 |
| `blend` | `blend(mode)` | how the layer joins what is below, the W3C compositing modes by their CSS names: `"normal"` (default), `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`, `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`. Blended in sRGB values, as vector and a browser do. A look's own `blend_mode` (image's spelling: `soft_light`) is inside the look and stays image's | `InvalidError`: another word |

### Masks

`mask(by, *, use="alpha", feather=0, invert=False)` multiplies the layer's alpha by a mask. Calling it again multiplies
another mask in.

| Argument | Meaning |
|---|---|
| `by` | a `Layer`: placed on the canvas by its own `fit` and `place`, drawn as it would be, and read as `use` says. Or a float array (height, width), 0..1: a map of this layer's own picture (a subject matte), which moves, scales and is cut with the layer |
| `use` | `"alpha"` or `"luminance"` (0.2126 R + 0.7152 G + 0.0722 B of the sRGB values, times the alpha: white keeps, black hides), for a `Layer`; an array is its own values and takes the default only |
| `feather` | the standard deviation, in design pixels, of a Gaussian blur of the mask's edge; 0 is hard |
| `invert` | 1 minus the mask (after the feather): type that passes behind a subject is masked by the inverted subject |

A mask layer's alpha includes its own masks, not its looks. Its parameters are addressed under the layer: `title.mask.dx`
(a second mask is `mask#2`). Errors: `InvalidError` for an array that is not 2-D, not 0..1 or of another picture's size
(checked at render, naming both sizes), `use` another word, `feather` below 0 or not finite.

### Looks

`look(recipe)` runs an image look on the layer. `recipe` is an image `Recipe` (anything with `to_dict()`), the dict it
writes, or a path to a recipe file. Looks apply in call order. See "Data in".

- **Space.** A look works on the layer's own picture, as fitted, not on the canvas: a vignette follows the layer's box, a
  centred warp its centre. A look on a group (`Layer.scene`) sees the whole group. This keeps a layer's result independent of
  the others, so it caches by itself.
- **Design pixels.** A `px` value of a look is a pixel of the scene's canvas, whatever the layer's size: 2 px grain on a
  small inset is as coarse as on the photo behind it. `render(size)` multiplies it, as image's `render(size=)` does.
- **Light that spills.** A look runs on the layer's colour over black, so light it adds past the shape (a bloom's halo) is real
  colour. A layer blended `"screen"` shows that light whole; any other blend cuts the layer to its own alpha, so a halo or a
  shadow outside the shape does not show. A glow is therefore a copy of the layer: `logo.look(bloom).blend("screen")`.
  Darkness outside a shape (a drop shadow) is a layer of its own ([composed_world.py](examples/composed_world.py)).
- **Maps.** A look's regions see the picture's `maps`. Scene runs no model: a subject or depth map is found by the caller with
  image and handed in.

## Data in

| Layer | Takes | Scene needs of it |
|---|---|---|
| `look(recipe)` | an image `Recipe`; its `to_dict()`; a recipe file path (read when called) | steps that are plain data. A `Picture` or `Lut` parameter (`to_dict()` leaves the object) is `InvalidError` naming the step; give a path in a recipe file |
| `Layer.drawing(source)` | a vector `Drawing`; its `to_json()` text or dict; a drawing file path (read when called) | a frame (`size`). A bare `Path` has no `to_json`: `TypeError`, wrap it in `btvec.Drawing(path)` |
| `Layer.picture(source)` | an array or a path | in JSON a path with a digest |

A look or drawing is checked when it is given, by the crate scene links, so a typo fails at the call naming the layer and the
step. A path given is read once, there, and embedded: a saved scene is self-contained. A picture, a font file or a map file
is not embedded; it is read at `render`.

**Version skew.** A saved vector document carries `"version": 1`; an image recipe carries a `format` in a recipe file (3
today; `Recipe.to_dict()` writes none, so scene stamps the format of the crate it links). A reader refuses what the linked crate
does not read, naming where it is, the version found, the versions read and the sibling crate's version:

```
FormatError: /layers/2/drawing: vector document version 2; this scene reads vector versions [1] (bullterrier-vector 0.1.0)
FormatError: /layers/0/look: image recipe format 4; this scene reads image formats [1, 2, 3] (bullterrier-image 0.9.0)
```

A scene itself carries `"version": 1`; scene's own format changes only by a new number, and a release that does so keeps
reading the old ones.

## Values

`scene.parameters()` returns `dict[str, Parameter]` in the order layer by layer: the layer's fields, its source's, its looks',
its masks'. The key is the address; the value is a frozen dataclass that holds the parameter format's fields (version 3) except
`name`: `kind`, `meaning`, `default`, `unit`, `min`, `max`, `cyclic`, `choices`, and `value`, what the scene holds now. A
tool that writes the format uses the address where the format says `name`.

| Address | Kind | Unit, range | Default |
|---|---|---|---|
| `<layer>.opacity` | float | fraction, 0..1 | 1 |
| `<layer>.blend` | choice | the modes above | `normal` |
| `<layer>.dx`, `.dy` | float | px, -1e6..1e6 | 0 |
| `<layer>.scale_x`, `.scale_y` | float | ratio, -1000..1000, not 0 | 1 |
| `<layer>.rotate` | float | degrees, cyclic | 0 |
| `<layer>.skew_x`, `.skew_y` | float | degrees, -89..89 | 0 |
| `<layer>.picture` | picture | | none |
| `<layer>.text` | text | at least 1 character | the content |
| `<layer>.color` | color | of a text or fill layer | the colour |
| `<layer>.look.<image address>` | as image | as image | as the recipe |
| `<layer>.look#2.<image address>` | | | the second look |
| `<layer>.drawing.<param>` | float | as the vector `Param` | its default |
| `<layer>.scene.<address>` | | | an address of the inner scene |
| `<layer>.mask.<address of the mask layer>` | | | `mask#2` for the second |

The first segment is the layer's name; the next is a field, `look`, `drawing`, `scene` or `mask`, and the rest is the sibling's
own address, verbatim (`bloom#2.radius`, `each.looks[0].gain`, `region.near.low`). Examples:
`title.dx`, `photo.picture`, `windows.look.swell.amount`, `logo_glow.look.halo.amount`, `logo.drawing.ring_on`,
`badge_b.scene.label.text`, `headline.mask.dx`.

`Parameter` follows the format's spelling (`meaning`, `min`, `max`); image's Python `Parameter` says `doc`, `minimum`,
`maximum`, and vector writes version-1 units; scene writes version 3 for both (`deg` is `degrees`, `cycle` is `cycles`,
`""` is `None`).

`with_values` rules:

- A value is a number (a `float`, an `int` or a numpy number; a whole-number parameter takes `3.0`), a colour
  (`"#rrggbb"`, `"#rrggbbaa"`, or `[r, g, b]` in 0..1), a string, a mode name, or for a picture a path, an array or `None`
  (a hole again).
- A value outside its range is refused, never clamped, naming the address, the range and the value; a cyclic one wraps.
- An address in an embedded look or drawing changes that layer's copy, checked afresh by the sibling's own rules.
- Equal scenes with equal values render equal pictures: a cache keyed by a layer's own inputs and values (C7) redoes only what
  a changed value reaches.

## Output

`render(size)`: `size` is the long side in output pixels (image's convention), a whole number from 1 to 65,000 with at most
2^27 pixels in all (a float32 RGBA array that size is 2 GiB; larger prints wait for bands and O1); another is `InvalidError`
naming `size`. The canvas keeps its aspect: a canvas of 720 by 1280 at `size=1280` is 720 by 1280 pixels; each design pixel
is `size / max(width, height)` output pixels.

The result is a new float32 array (height, width, 4), 0..1, rows top to bottom, straight alpha, transparent where nothing is
drawn. Colours are sRGB values. The same scene, values and size give the same array on the same machine and library versions;
a smaller size shows the same picture in less detail. `render` reads the files the scene names (pictures, fonts, maps), writes
none, and keeps no state between calls. A hole is `InvalidError` naming `<layer>.picture`; a file that is missing, unreadable
or whose digest differs is `ReadError`; an error from an embedded look or drawing at draw time is re-raised as `InvalidError`
naming the layer, with the sibling's message.

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
     "fit": {"box": [0, 0, 1280, 720], "how": "cover", "anchor": [0.5, 0.5]}},
    {"name": "band", "fill": "#101018", "fit": {"box": [0, 480, 1280, 240], "how": "stretch"}, "opacity": 0.82},
    {"name": "title", "text": "Episode title", "font": "bricolage_grotesque", "size": 84, "color": "#ffffff",
     "width": 1080, "axes": {"wght": 800}, "place": {"dx": 100, "dy": 590}}
  ]
}
```

A layer holds its source (`picture`, `drawing`, `text`, `fill`, `scene` or `kind` and `options`), `fit`, `place`, `opacity`,
`blend`, `masks` (each a layer, or `{"map": ...}` and its options) and `looks` (each `{"format": 3, "recipe": {...}}`), a key
only where it differs from its default. A drawing is the vector document inline, a scene layer the inner document inline. A
raster, a font file or a map file is `{"path": ..., "sha256": ...}`, relative to the scene file's folder when it can be (a
mismatch is `ReadError` at `render`); a bundled font is its name. A path where a look or drawing belongs
(`"looks": ["looks/night.toml"]`) is read when the scene is loaded and written inline by the next `to_json()`. `FormatError`
carries the JSON Pointer of the value and what is wrong; it is also a `ValueError`.

## Errors

| Error | When | Carries |
|---|---|---|
| `BullterrierSceneError` | the base of every error scene raises for a caller's input | |
| `InvalidError` | a value or argument scene refuses, also a `ValueError`; at the call that got it, else at `render` | the argument or address, the rule and the value; for an embedded look or drawing, the layer and the sibling's message |
| `FormatError` | a document `from_json` cannot read; also a `ValueError` | the JSON Pointer; a version skew names versions as above |
| `ReadError` | a file the scene names cannot be read, or its digest differs; also an `OSError` | the layer, the path, and for a digest both digests |

A wrong type in the calling code (`Layer.drawing(path)`, `add(scene)`) is a plain `TypeError`. Plain numbers are checked when a
call is made; a hole, a map's size and files when the scene renders. Depth is bounded (`MAX_DEPTH`, `MAX_LAYERS`) so no walk
needs more than a 256 KB thread stack.

## Conventions

- **Names in the field's own terms.** `dx`, `dy`, `anchor`, `feather`, `opacity`, `color`, `box`; never `strength`,
  `factor`, `amount`, `val`. One name per concept: a pivot is `anchor`, an offset `dx`, `dy` (as vector's `translate`), a colour
  `color`, a position inside a box a fraction 0..1.
- **Units.** Design pixels for lengths, degrees clockwise for angles (wrapping), fractions 0..1 for opacity and anchors,
  ratios for scale. Time does not exist.
- **Arguments.** What a call cannot do without is positional (`opacity(0.5)`, `mask(by)`, `fit(box)`); every option is
  keyword-only. `place` has no required argument, so all of it is keyword-only.
- **Colours** are `"#rrggbb"` or `"#rrggbbaa"`, one spelling in all three libraries (vector refuses `"#rgb"`).
- **Alpha and colour.** Straight alpha at every boundary, premultiplied inside; blends in sRGB values.
- **A look and a drawing keep their own words.** Blend modes of a layer are CSS names (`soft-light`); inside a look image's
  stay (`soft_light`).
- **How a method is documented.** One line that states the promise in the field's terms; each argument with its unit, range and
  default; each error and when. Never how it works.

## Languages

The same names in each language's case: `snake_case` in Python and Rust.

| | Python | Rust |
|---|---|---|
| Constructors | `bts.Scene(1800, 2400)`, `bts.Layer.picture(photo)` | `Scene::new(1800, 2400)?`, `Layer::picture(Raster::rgba(w, h, pixels)?)`, `Layer::picture_file(path)`, `Layer::drawing(json)?`, `Layer::fill("#f2ece0")?`, `Layer::scene(inner)` |
| Data in | a look or drawing object by its `to_dict()` or `to_json()`; an array | a `&str` or `serde_json::Value` of the document; a scene-owned `Raster` (width, height, `f32` RGBA) and `Map`. The pinned image and vector crates' types never appear in scene's public API: they stay inside two adapter files and out of its semver |
| Options | keyword arguments | a value named after the call, built with `new` and setters: `Place::new().dx(100.0).rotate(Deg(-4.0))`, `Text::new("bricolage_grotesque").size(84.0)`, `Mask::new(by).feather(8.0).invert()`; the required argument alone converts into it: `layer.mask(by)` |
| Modes | strings: `"cover"`, `"screen"`, `"luminance"` | enums: `Fit::Cover`, `Blend::Screen`, `Use::Luminance` |
| Values | `scene.with_values({"title.text": "Pilot"})` | `scene.with_values(&Values::new().set("title.text", "Pilot"))?` |
| Output | a numpy array | a `Raster` of the output size; a caller encodes it (PNG is O1) |
| Errors | exceptions | `Result<_, bullterrier_scene::Error>`; a call that can fail returns it (`named`, `opacity`, `look`, `mask`, `render`...), a scene never defers its first error. `Error` is `#[non_exhaustive]`: `Invalid { address, message }`, `Format { pointer, message }`, `Read { path, message }` |
| Plugin kinds | an entry point | a `Source` trait with the one question "draw this rectangle at this scale", registered in a `Registry` |

[cover_template.rs](examples/cover_template.rs) is the cover template in Rust.

## What the demos asked

Every row of "What the demos and the experiment asked of scene" in [roadmap.md](roadmap.md), and where an example covers it.

| Need | Covered by |
|---|---|
| Move, scale and stand a cut-out subject on a floor | `fit` with `anchor=(0.5, 1.0)`, [composed_world.py](examples/composed_world.py):41-48 |
| An effect on one layer only: the logo's glow must not light the windows; the sun glows, the dog does not | a copy of the layer with a look and `screen`: [mill.py](examples/mill.py):79-83, [composed_world.py](examples/composed_world.py):54-56 |
| Contact shadow and floor reflection of a cut-out | the same layer flattened and mirrored by `place`, masked: [composed_world.py](examples/composed_world.py):58-66 |
| Reframing to the delivery shape without an outside crop | `fit(box, "cover", anchor=...)`: [mill.py](examples/mill.py):58-60 |
| One colour typed once, for vector, image and the caller's code | in part: a scene's own colour is one parameter ([cover_template.py](examples/cover_template.py):18, :40); inside a look or a drawing it waits for a colour reference in image and vector (Open questions 1) |
| A region of a layer as the target of an effect: the sign, each window | a region of an image look as a map, [mill.py](examples/mill.py):46-50, :99; a drawing as a mask, :96-101 |
| An effect's space: the layer's own picture, or the canvas | the layer's own picture; decided under "Looks", "Space" |
| A static layer renders once | equal values render equal pictures; only `headline` changes: [frame_sequence.py](examples/frame_sequence.py):32, [mill.py](examples/mill.py):130 |
| Values per frame arrive as `scene.with_values(...)` | [frame_sequence.py](examples/frame_sequence.py):32, [mill.py](examples/mill.py):108-130 |
| What scene does not take on: facade detection, prompted segmentation, footage mattes, shot cuts, a timeline | "What this API deliberately does not have"; the matte is the caller's: [mill.py](examples/mill.py):24 |

## What this API deliberately does not have

- **No class per kind.** One `Layer`, one mode string for blends, one `mask`, one `look`; no group type (a scene is a group), no
  mask type (a layer is a mask), no gradient (vector's).
- **No mutation.** Every call returns a new value; a layer used twice is the same layer twice.
- **No callbacks in a scene.** A scene saves as JSON and renders where no caller's code runs. A plugin kind is named, with
  plain options.
- **No time.** No clock, easing or signal: a parameter is a state; the caller sets it and renders again.
- **One way to set values.** No `render(size, values)`, no keyword form, no separate `pictures=`: footage is a picture
  parameter.
- **No implicit IO.** `render` reads the files the scene names and writes none; `to_json` writes nothing, the caller saves it.
- **No models.** A subject or depth map is found by the caller with image.
- **No global state.** No current scene, canvas, font or random generator.
- **No configuration objects.** Options are keyword arguments of the call they shape.
- **No reorder or remove.** A scene is rebuilt from its layers (`Scene(w, h).add(*layers)`).

## Decided after review

- **Immutable values** (`Scene`, `Layer`): simple cache keys, no aliasing.
- **Looks and drawings enter as data**, embedded in a saved scene; a path is a read-only convenience. Rust takes serialised
  data, not the sibling crates' types.
- **One way to set values**, `with_values`; the clock stays outside.
- **Addresses** are the layer's name and the sibling's address verbatim; the format's `name` rule applies to the layer name.
- **Rasters** are arrays in code and a path with a digest in JSON.
- **Output** is a float32 array, PNG is image's, `size` is the long side.
- **A group is a scene; a mask is a layer** (or a map of the layer's own picture).
- **Placement is `fit` then fixed fields (`place`)**, not a chain of verbs, so every placement number has an address.
- **Screen shows what a look adds past the shape**; other blends cut to the layer's alpha.
- **Where the entire work ends:** A1, C1 to C7, M1 to M3, O1 (PNG only, through image's save), P1 (a Rust command line that
  the Python console script runs, as image does) and the final poster. B1 (bands) is deferred, since it needs vector's window
  render and image effects that take an origin and a canvas size; the interface does not change when it arrives.

## Open questions

1. **One colour across scene, looks and drawings.** Image and vector validate colours as hex when they build, so a scene cannot
   push a name into them. Needs a colour reference from both (C6). Until then a colour is a Python constant.
2. **A layer used twice has its parameters twice** (a logo and its glow): the caller sets both, as `frame_values` in
   [mill.py](examples/mill.py) does. A link between layers would be a new concept; decide at C6 if examples keep asking.
3. **Alpha through looks.** A look does not change a layer's alpha, so dark light past a shape (image's `drop_shadow`) is cut;
   only `screen` shows added light. Lifts when image carries alpha through its effects.
4. **A region as a map.** Image shows a region as a stage of `trace`, found by its label ([mill.py](examples/mill.py):46); a
   named call returning the map is asked of image.
5. **Limits are placeholders** (`MAX_LAYERS`, `MAX_DEPTH`, 2^27 pixels) until C1 measures them.
6. **Image's `Recipe.to_dict()` carries no `format`**; scene stamps the linked crate's number until image writes it.
