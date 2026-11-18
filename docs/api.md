# The code API

Status: A1, designed from the example programs in [examples/](examples/); C1 (the walking skeleton), C2a (`fit` and the
affine `place`), C2b (`perspective`), C3 (masks, maps and text), C4a (blend modes and light), C4b (references), C4c (scenes as layers), C5a, C5b-lite and C5c (looks on every layer, design pixels, room for light), C6a (parameters), C6b (the scene as JSON, `==`), C7 (the cache, the plugin seam in Rust), C8 (layer kinds by name: the registry, Python plugins, their JSON), O1 (PNG and PDF) and P1 (`bt-scene`) are built, the rest is not. What they make real, and where they differ from the contract below, is "What is
built". Every other signature is what a later batch makes real and each example is the acceptance material of the batch
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

**C2b** adds `Layer.perspective(corners)` (Python, four (x, y) pairs) and `Layer::perspective([[f64; 2]; 4])` (Rust).

**C3** adds `Layer.picture(source, maps=)`, `Layer.mask(by, use=, feather=, invert=)`, `Layer.text(content, font=, color=,
**text_options)` and `MAX_DEPTH` (Python), and in Rust `Map::new(width, height, values)`, `Layer::{map, map_file, map_hole, mask,
text}`, `Mask::new(by).using(Use::Luminance).feather(8.0).invert()` (`using`, because `use` is a keyword; `layer.mask(by)` takes
a `Layer` or a `&str` alone), `MaskBy`, `Use`, `Text::new(font).color(c).option("size", 84.0)` (`Text::from_file(path)` for a font
file), `Layer::text(content, &text)` and `MAX_DEPTH`.

**C4a** adds `Layer.blend(mode)` and `Layer.as_light(on=True)` (Python), and in Rust `Blend` (`"multiply".parse::<Blend>()`, the sixteen CSS
names), `Layer::blend(Blend::Multiply)` and `Layer::as_light(true)`, both returning the layer (no refusal at the call). 

**C4b** adds `Layer.of(name)` (Python) and `Layer::of(name)` (Rust: it returns the layer, no `Result`), and `Scene::add` finds the names.

**C4c** adds `Layer.scene(inner)` (Python) and `Layer::scene(inner: Scene)` (Rust: it returns the layer, no `Result`; what it can be refused for is found by `Scene::add` and `Layer::mask`).

**C6a** adds `Scene.parameters()` and `Scene.with_values(mapping)` (Python; `Parameter`, a frozen dataclass; `Layer.picture(None)`, a hole), and in Rust `Scene::{parameters, with_values}`, `Parameter`, `Kind`, `Value`, `Values` (`scene.with_values(&Values::new().set("title.text", "Pilot"))?`) and `Layer::picture_hole`. "Values" says what is listed; the rows below say where it differs from that.
**C6b** adds `Scene.to_json()` and `Scene.from_json(text, *, base=None)` (Python) and `Scene::to_json()` and `Scene::from_json(text, base)` (Rust, `base: &Path`, empty for the working folder), and `==` on `Scene` and `Layer` (`PartialEq`). "Saved scene" is the document.

**O1** adds `Raster::write(path)` and `Raster::write_pdf(path, &PdfOptions)` (Rust only), `PdfOptions`, `Error::Write`: "Output" says the rounding, the formats and what the PDF is.

**P1** adds the command line `bt-scene` (`crates/cli`, `bullterrier_scene_cli::main(args, version, out, err)`; the Python console script and `python -m bullterrier_scene` run it in-process through `_core.main`): `render SCENE OUT [--size N] [--set ADDRESS=VALUE]... [--base DIR] [--dpi N] [--bleed MM] [--bleed-fill repeat|mirror] [--crop-marks] [--registration-marks] [--flatten] [--cmyk-profile FILE]` (`OUT.pdf` writes a PDF; the PDF flags with another extension are exit 1 naming the flag), `parameters SCENE [--json]`, `validate SCENE`; exit 0 success, 1 the command failed (`error: <message>` on stderr, the library's own message), 2 wrong usage. [cli.md](cli.md) is a session. `--set` reads VALUE by the kind of the address (a number, `true` or `false`, text, a picture path); an address the scene does not have goes to `with_values`, which lists the nearest. `--size` defaults to the canvas's long side. `--base` defaults to the scene file's folder. `parameters --json` is `[Parameter.to_dict(), ...]`. `validate` renders at size 1 and writes nothing: it reads every file, checks its digest and draws every layer, so a hole, a map of another size and a look the sibling refuses fail too.

**C7** adds the cache (no call: `render`, and everything derived by `add` and `with_values`, gets faster; "Cache" below and in "Output") and the plugin seam in Rust: the trait `LayerKind` (`name`, `options`, `draw(area, scale)`) and `Layer::make(Arc<dyn LayerKind>)`. A layer made by `Layer::make(kind)` is refused by `to_json`, as an array is, and is equal by name and options; C8 gives a kind a name.

**C8** adds layer kinds by name. Rust: `LayerKinds` (a registry value, not a global: `new`, `register(Arc<dyn LayerKindFactory>)`, `get`, `names`, `make(name, &options) -> Layer`), the trait `LayerKindFactory` (`name`, `identity`, `create(&options) -> Arc<dyn LayerKind>`), `Scene::from_json_with(text, base, &LayerKinds)` (`from_json` reads through an empty registry) and `MAX_PIXELS`. Python: `bts.Layer.make(kind, /, **options)`, `bts.register_layer(kind, draw)`, the decorator `@bts.layer_kind(kind)`, `bts.layer_kinds()` (sorted names) and the entry-point group `bullterrier_scene.layers` (loaded on the first of `layer_kinds`, `Layer.make` and `Scene.from_json`, never at import); the module `bullterrier_scene.plugin` holds `GROUP` and `isolated()` for a plugin's own tests. "Plugin kinds" in Sources has the rules.

**B1** adds banded rendering. Rust: `Scene::render_bands(size, band_rows, sink)` (`sink(rows, &Raster)` is given each band, top to bottom), `Scene::picture_size(size)` (the columns and rows of the picture), `Scene::render_to_png(size, path)`, `Scene::render_to_pdf(size, path, &PdfOptions)` and `BAND_PIXELS` (2^24). Python: `Scene.render_bands(size, band_rows, sink)` (`sink(first_row, band)`). `bt-scene render` writes a picture of more than 2^24 pixels by bands. "Output" has the rules, "Banded rendering (B1)" in `design-decisions.md` the design and the numbers.

Nothing else below exists yet. Each call is as written below, except:

| Where | Built | Changes with |
|---|---|---|
| Boxes | A picture's box is its pixels and a drawing's its frame, one design pixel each, at the canvas origin; a fill covers the canvas; `fit` and `place` move them. What lies outside the canvas is cut. The picture covers the canvas exactly: the long axis has exactly `size / long side` output pixels per design pixel; the short axis has its length rounded up to whole pixels and the picture is stretched onto them, so there a design pixel is `rows / height` (or `columns / width`) output pixels, within one pixel over the whole canvas of `size / long side`. That is what lets a layer that covers the canvas cover it whole at any size. **Limit:** on a thin canvas the design pixel is visibly not square (a canvas of 100 by 3 at `size` 7 is 7 by 1 pixels: 0.07 output pixels along x and 1/3 along y), and a turn or a skew of a layer on it is sheared by that ratio; draw such a canvas at a size where its short side is whole. A box that ends between two output pixels covers the pixel it ends in in part (a drawing draws its own edge: "Drawings") | |
| Resampling | One warp per layer ([design-decisions.md](design-decisions.md), "Placement"): bilinear on premultiplied colour; where the map shrinks the picture, the average of what the output pixel's footprint covers (an exact area average where the map keeps the axes, a lattice of taps over the footprint elsewhere), at any skew; no negative weight (a bicubic upgrade must restore a clamp); nothing outside the fitted box is drawn and an edge between two pixels is soft by coverage; a move by whole pixels, a quarter turn and a flip are exact copies | |
| `fit`, `place` | `anchor` outside 0 to 1, and `dx` or `dy` outside -1e6 to 1e6 (the range of the parameters), are `InvalidError` naming the field. A call that sets a number the call refuses leaves the layer as it was. `fit` is resolved when the scene renders | |
| `perspective` | Corners beyond -1e6 to 1e6 or not finite, and corners that are no strictly convex quadrilateral, are `InvalidError` naming `corners`; a mirrored (counter-clockwise) one is accepted. A layer under one is drawn at the largest scale the quadrilateral gives any corner of its window along each axis, cut down by one factor to the 2^27 pixels of a layer where it would pass them (not refused: the near side loses sharpness; a layer drawn by window would lift that, and a drawing is not yet, `future.md`). A parallelogram is affine and takes the exact path: an axis-aligned one is what `place` makes, bit for bit where its numbers are exact (whole, or fractions of a power of two) and within the blend's rounding (1e-5) where they are decimals | |
| A look | Every layer takes one (C5a). It runs on the raster the source drew at the size the layer has: `output scale x fit factor x scale` (see "How a layer is drawn"), on its straight colour with the alpha left as it is (it is the source's, put back after the look). A look's `px` are image's design pixels ("Looks", Design pixels). Image refuses a look on a raster of more than 2^26 pixels, in its words | |
| Design pixels (C5b-lite) | `Finish::run` hands image `Options::with_canvas(w, h)` with the scene's canvas as this layer is drawn: its design size times the density the layer is drawn at without the fit (`output scale x place.scale`, per axis, rounded up to whole pixels; a group's inner layers: the group's drawn density, `Request::scale`). Image's design scale is the canvas's longer side over 480, so a design `px` is one pixel of the canvas's 480 across its long side (`px="canvas"` hands image a canvas of 480 canvas pixels instead: `Request::canvas_of`), the same number of output pixels on every layer of one canvas, whatever the layer's size or `fit`; `place.scale` 2 doubles it; a layer larger than the canvas (a drawing's whole frame) is looked at the canvas's scale. **Limits:** the canvas side is whole pixels, so at a fractional `place.scale` the design scale is that of the rounded canvas (within half a pixel in the canvas's long side); an effect that places itself in its picture (a vignette's centre) still sees the layer's raster as the whole, as the contract says. a light layer's look has room (next row) | C5b room |
| Room (C5c) | A **light** layer's looks run on its raster grown by the room image declares for them: `Recipe::spread` of each look at the canvas long side handed to image (`Options::with_canvas`), summed along the looks, per side (top, right, bottom, left), rounded up to whole pixels. The grown raster (transparent black round it) is what the layer lands: the box stays where it was and the halo shows past it; the warp lets the whole grown raster through, the layer's masks by a layer are multiplied where it lands, its masks by its own map see the map continued by its nearest edge pixel. **A layer that is not light has no room**: it keeps its source's alpha, so a halo would be cut, and black room would pull a blurred edge dark (image's own edge rule keeps the picture's colour); it is looked at its own size, as before. **No room (not an error) where:** a look is `Unbounded` in image (a warp about the centre: `swirl`, `pinch_bulge`, `lens_distortion`, `radial_blur`, `kaleidoscope`, `polar`; whole-picture statistics: `auto_levels`, `equalize`, `color_transfer`, `dehaze`, `edges`, `dither`, ...) or the grown raster would pass image's 2^26 pixels; a look that spreads none (a grade, a vignette, grain) leaves the layer as it was, bit for bit. A look that mixes a halo with an effect that places itself by the picture's bounds (a vignette, a centre `x`/`y`, `chromatic_aberration`, noise numbered by place, `tone_map`) places that effect by the layer's box (image's `Options::with_room`); **limit:** such a step after a halo step does not reach the halo, so order the halo steps last | C5c |
| `format` of a look | `Recipe.to_dict()` writes the lowest `format` the recipe needs, and image's reader takes it with the others it publishes (`RECIPE_FORMATS`). A `format` it does not read is a `FormatError` under `/look/format` naming the version found and the versions read, not the crate's version, which neither crate publishes | |
| A drawing | `inputs` (font bytes) is not built: no layer needs a font yet. A document vector cannot read is `FormatError` under `/drawing` in vector's words, a version skew included, without the crate's version. A banded render draws a drawing for the rows a band reads (vector's window render, the crop of the whole render byte for byte); where vector refuses a window (a layer past 65,000 pixels a side or 2^27 in all) the whole render is refused too, `InvalidError` naming `window` | text layers |
| Errors | The address of an error of a layer in a scene starts with its name (`photo.look`), a layer without one `layer1`; at the call that made it, before the layer has a place, it is the field (`look`) | |
| Python paths | `Layer.picture(Path)` is read at `render`; `Layer.drawing(Path)` and `Layer.look(Path)` at the call | |
| Equality (C6b) | `==` on a `Scene` or a `Layer` is **the same document** (`Form::Compare` of `document.rs`: the document `to_json` writes, with a picture or a map held in memory by the SHA-256 of its pixels, a file by where it is, and no digest). It never renders and never reads a file. A look and a drawing are equal by what their crates write of them (image's `Recipe` as JSON, vector's canonical document), so a colour's case, a look's `format` number a recipe does not need and a look built from a dict or a file do not matter. Equal scenes list equal `parameters()` and render equal pictures; the converse is false (two documents may draw alike). **Limit:** the pixels of a held picture are hashed once, the first time a scene asks (`==` or the cache: a 12 megapixel picture, 192 MB, 0.1 s on eight cores), and the digest stays with the picture however many scenes hold it; a scene with a drawing that vector cannot write (past its document limits) is equal only to a scene that fails alike. In Python a `Scene` and a `Layer` are unhashable |
| Maps | A map is a float32 (height, width) array of 0 to 1 (`uint8`, `uint16` and `float64` too; a `bool` array is `TypeError`, naming the dtype: `array.astype(np.float32)`), a `Path` to a grey picture read at `render`, or `None`, a hole. Maps are given with the picture (`maps=`; Rust `Layer::map` and `map_file` after the constructor) and are parameters of kind `picture`, `<layer>.map.<name>`. A hole is `InvalidError` at `render` naming `<layer>.map.<name>` until `with_values` fills it; a map of another size than its picture is `InvalidError` there, with both sizes. A name is letters, digits and `_` starting with a letter. A look is handed the maps its recipe can be given (`subject`, `depth`, a `given` region) through image's `Photo::with_map`, resampled to the size the look runs at; a map the recipe does not know (`matte`) is kept for masks and not given | |
| Masks | Stages 4 and 6 as "How a layer is drawn" says. A mask by a map name is checked at the call (`mask` is `InvalidError` for a name the layer does not have yet: give the map first) | |
| Mask layers | A mask layer is drawn as any layer is, on the canvas grid, and only its alpha, or its brightness times its alpha, is kept. Its opacity multiplies the mask. A name (`named`), a non-normal blend and `as_light(true)` on a mask layer are each `InvalidError` naming `mask` at the `mask` call: a name would collide with the lookup of a reference (C4b), and a blend and a light have no meaning in a mask (`blend("normal")` and `as_light(False)` say nothing and are accepted). Its own masks and looks apply; `Layer.of` as a mask layer is a reference ("References"), whose own place and looks apply | |
| References | `Layer.of(name)` (and `Layer::of(name)`) is a layer that holds a name and what it adds: `look`s, `mask("map")`s and an extra `place`; it has no source (`InvalidError` at the call naming `fit`, `perspective` or `map`: "a reference has no fit of its own", with the name it refers to). **Inherited and owned** is the split of `Layer` into a body (source with its looks and own-map masks, `fit`, `place` or `perspective`: stages 1 to 5) and a join (layer masks, light, opacity, blend: 6 to 9, the reference's own); the code that resolves a reference has the named layer's body alone, so it cannot inherit the join. **`Scene::add` finds the names** among all the layers of the scene and of the call, so the order in the call is free and a reference is stacked where it was written (`add(glow, sun)` draws the glow below the sun); a name that is no layer, or is of a layer added by a later call, is `InvalidError` naming `<layer>.of`. It then resolves each reference, in the stack and in mask layers, into the body it draws (the named layer's, its own looks run on the named layer's source after that layer's, its own-map masks by the named picture's maps, its extra place appended to the extras the chain has), and stores the layers as they are drawn beside the layers as they were added (`Scene.layers` returns the second: a reference stays one). What the named layer cannot take is found there too: a mask by a map the layer lacks is `InvalidError` naming `<layer>.mask`; every layer takes a look. A reference's own looks are checked at the call (image's words); a reference given the same layer in two scenes names the layer of each. **Cycles:** a chain that leads back to itself is `InvalidError` naming `<layer>.of` and the cycle (`a -> b -> a`), found by an iterative walk over the references of every layer including those in its mask layers. **A cycle through a mask is refused although it would end:** a reference takes the body of the layer it names and none of its masks, so `a.mask(Layer.of("b"))` with `b.mask(Layer.of("a"))` draws, and `a.mask(Layer.of("a"))` (a layer cut by a copy of itself) would too; both are refused so that looks on every layer, which may make a mask part of what a reference draws, add no cycle later: a refusal can be relaxed, a cycle accepted cannot be refused. Use a second copy of the layer for a layer masked by itself. **Error addresses:** a reference's own-map masks are numbered by its own `mask` calls (`mask`, `mask#2`), as the named picture's are by its own, so an error of a feather too large for a map names `mask` and not which of the two layers' | |
| Groups | `Layer.scene(inner)` is a layer whose source is a scene held by value (`Arc`, so a scene used many times is held once; a scene is immutable, so no scene can contain itself and there is no cycle through scenes). Its box is the inner canvas, one design pixel each, at the canvas origin like a picture. **How it is drawn:** the layer's `Layout` asks for the whole box at the density the layer will have (`output scale x fit x place.scale`, per axis, as a drawing is) and the group answers by drawing its inner layers onto a transparent premultiplied backdrop of that raster through the same internal entry as `Scene::render` (`render::draw`: the whole canvas is the case where the grid is the canvas); the finished raster, as straight alpha, is then drawn and joined as any layer is, so `fit`, `place`, `perspective`, masks, `blend`, `opacity` and `as_light` act on it as one picture. **Isolation** is that backdrop: an inner layer's blend meets the group's own content (over a transparent backdrop a `multiply` layer is its colour), the group's opacity fades the finished picture once (two opaque children overlapping in a group at 0.5 show the top child at 0.5 over what is below, `0.5 top + 0.5 base`), and the group's own blend meets what is below it. **Pixel phase:** like a drawing, the group's raster starts at the fraction of a pixel its box starts at when the placement only moves and zooms (`Request::phase`, now also handed to the inner layers as the origin of their grid, `Target::offset`), so a group at a fractional place is its children at that place, not resampled again (picture, drawing and fill rows, bit for bit within 2e-6, the rounding of one trip through straight alpha). **Limits:** (1) a layer that ends at the group's box ends there by its own coverage, as in a scene drawn whole, and the outer warp does not cut the box again when the whole box is shown, so a layer that overflows the box shows in the pixel its edge falls in (less than a pixel past the box); under a `fit` that crops (`cover`) the warp cuts the window, and an edge that coincides with the box edge is cut twice, as a drawing's is. (2) In `render` the whole box is drawn whatever part `fit` shows (as a drawing is); on a band (B1) only the rows of its raster that the band reads are drawn, so a group zoomed past 2^27 pixels is drawn by `render_bands`. (3) **A look's `px` inside an inner scene are the inner canvas's design pixels, or its canvas pixels with `px="canvas"`** (a badge looks the same at every use, at every size and in every place); a look on a group runs on its finished, isolated picture at the size the group is drawn at (C5a). (4) A translucent group goes through straight alpha: a group alone equals its scene's render bit for bit where the picture is opaque and within 2e-6 where it is not. (5) A group placed or turned equals the inner render put there as a picture within a stated tolerance (the group is drawn at its placed scale, the picture is resampled again): on a ramp, 0.005 where both cover the pixel and 0.3 in the alpha of a turned edge (0.24 seen at 17 degrees and 0.9); a group moved by whole pixels at one output pixel to the design pixel, and a group scaled by a whole factor, are exact. (6) A reference to a layer that is a group draws the group at the composite density, and shares the raster with the group where that density, the phase and the inner scene are the same (C7) |
| Extra place | A reference's `place` is applied after the named layer's placement, about its own `anchor`, as fractions of the bounding rectangle of the fitted box as the named layer places it (a rotated layer: the rectangle round the tilted quadrilateral; a `perspective` layer: round its quadrilateral); `Layout` holds the list of extras, one for each reference of the chain, each about the rectangle of what came before. The composite is one warp. **After an affine placement** the extra is an affine map in output pixels applied to the named layer's map (`Affine::then`): a move after a move, a quarter turn after a quarter turn, a flip after a flip, a halving after a doubling are the placement of their sum bit for bit (the entries are 0, 1 and sums of whole numbers), and an extra move after any place adds exactly its `dx`, `dy` to the translation (the pivot is added as `pivot - matrix x pivot`, 0 for a move, so its rounding does not enter: a spread of 200 inexact placements is tested); turns that are not literal quarter turns compose within rounding (1e-4 of a level in the tests) and are **not snapped** to the exact class: a turn by 30 degrees then 60 is the general kernel, not the axis-aligned one, though its pixels agree with a quarter turn's to 1e-4 (the plan said to classify the composite by the relative tolerance, which is what the perspective path does by inheritance; a snap of near-zero entries would also have to snap entries near 1, or the copy is not exact, and gains no pixels). **After a perspective** the extra is applied to its homography, a 3 by 3 product, so a perspective stays one and a parallelogram, whose last row is exactly 0 0 1, stays affine on the exact path (C2b's relative-tolerance rule decided it for the quadrilateral and the product keeps the row). **The drawn density follows the composite:** the source is asked for `output scale x fit x place.scale x the zoom of each extra` (a halo scales with the extra `scale`) and for a perspective the largest scale of the composite homography; `aspect`, flip, skew and turn of an extra are the warp, not the density, so an extra `aspect` above 1 enlarges what was drawn for aspect 1 (soft, as `aspect` of a layer is). **The pixel phase** of a drawing at whole pixels includes the extras that only move and zoom | |
| Blend | Sixteen modes, `Blend` by CSS name; any other word is `InvalidError` naming `blend`, listing the names and, if it is near, what it may have meant: the names it begins (lower case, `_` read as `-`: `"lumi"`, `"Multiply"`, `"soft_light"`), else those fewest edits away within a third of its letters, else none (`"bogus"`, `""`); `Blend` prints its CSS name (`Display`). A layer with a mode other than normal, or light, is joined pixel by pixel where the warp lands it (no canvas-sized raster), by the compositing equation of the W3C Compositing and Blending Level 1, section 9, on premultiplied floats: `Cs' = (1 - ab) Cs + ab B(Cb, Cs)`, `co = as Cs' + (1 - as) ab Cb`, `ao = as + ab (1 - as)`, with `Cb` the premultiplied backdrop over `ab` (0 where `ab` is 0) and `as` the source's alpha times its masks and its opacity. The separable modes are section 10.1, the four that mix channels section 10.2 (`Lum` 0.3, 0.59, 0.11, which are not the Rec. 709 weights of a luminance mask). A channel of the result is held between 0 and the result's alpha: in real numbers a clip lands exactly on 0 or 1 and a premultiplied colour never passes its alpha, rounding misses by ulps (-7e-9 from `luminosity`; 1.0000001 where a channel of 1 meets a source of 1: eight rows pin it). **NaN, fixed (found by the review):** `color` and `luminosity` gave NaN for 14 of the 256 greys because `ClipColor` measured the brightness of a colour that `SetLum` had just moved to a brightness (a grey's, an ulp off its level in 32 bits), the colour fell an ulp below 0 with its measured brightness equal to its least channel, and the quotient was 0 over 0. `ClipColor` now takes the brightness `SetLum` aimed at, a number from 0 to 1 (`Lum` is, by monotone rounding: white's sum is exactly 1), so its denominators are strictly positive whenever the branch runs. Every grey over every grey is swept in all sixteen modes at three backdrop alphas and two source alphas (a number from 0 to 1, no colour without alpha), every near-grey (a level, each channel an ulp up or down) over every grey and back in the four modes that mix channels, the four modes are held to the specification's identities on greys, and a property test over-samples greys, near-greys and corners. A normal layer that is not light runs the code it ran before. Blended in sRGB values; linear light is not offered. **Measured against vector's own blend** (the twelve separable modes, a grid of backdrops and sources that crosses every branch): within 2 levels of 255 for an opaque source (the largest seen 1.86, `exclusion`) and 2.5 for a translucent one (2.16, `hard-light`; `color-burn` 2.14: 115.9 against 118 where the W3C formula gives 115.86): vector draws in 8 bits with a rounded division by 255 in each product and holds a translucent source premultiplied. The comparison cannot see a boundary that moves by a level, so the closed forms of the tests do: soft-light's two boundaries (a quarter of the backdrop, half of the source) and `ClipColor`'s thresholds (a channel 1e-4 beyond 0 or 1) are pinned by hand-worked rows; the four non-separable modes equal constants worked out by hand from the pseudo-code to 2e-6 | |
| Light | `as_light` is stage 7 on each pixel as the warp lands it: the pixel's premultiplied colour `c` (straight colour times alpha) is kept, its alpha becomes the largest of `c` (0 where `c` is 0) and its straight colour `c` over that alpha. The masks and the opacity then scale that alpha (a mask first or last gives the same: light is linear in alpha). A look runs before it. A fill and a drawing are light too. With a look that spreads (C5c), the halo past the box counts too ("Room") | C5b, C5c |
| Feather | `feather` is 0 to 1,000,000 design pixels (`ranges.rs`, as far as a layer moves), the standard deviation of three box passes within 0.0099 of the Gaussian's distribution function from a sigma of 1.5 output pixels up (a dense scan from 1.5 to 41 gives at most 0.00985; below it the kernel is a few pixels wide and off by up to 0.012 at sigma 1, 0.049 at 0.5 and 0.013 at 0.2, so a feather under 1.5 output pixels is a soft edge and not a Gaussian), reaching about 3 sigma and no further ("The feather" in [design-decisions.md](design-decisions.md)); 0 is no blur, exactly. It is in design pixels of the canvas for a mask layer, and in design pixels of the layer as it is fitted, before `place`, for a map (so `place`'s `scale` scales it, as it scales a look's halo). A canvas mask whose plane (the canvas, the margins of the masks nested in it and the reach of the blur on every side) would pass 2^27 pixels is `InvalidError` at `render` naming `<layer>.mask.feather` and the limit: a sigma of about 1,900 output pixels at most on a canvas of 100 by 100 pixels, and less as the canvas grows. A map makes no plane, so a picture of any size is masked and blurred: only a line that would pass 2^24 pixels with the reach of the blur (a feather of 1,000,000 design pixels on a layer drawn a thousandth of its size) is refused, naming the feather; a mask with no feather is never refused for its size (it costs one copy of the picture). The blur of a large map costs time in proportion to the reach (a feather of 1,600 on 2,000 by 2,000 pixels takes seconds) The blur is of the values the mask reads, then `invert` takes 1 minus them: the two commute (the kernel is linear and sums to 1), so the order is the reader's, not the render's | |
| Mask edge | **Trap: a mask layer that is a picture the size of the canvas ends at the canvas edge, so feathered it darkens the border (0.5 at the edge), whereas a fill covers the margin and a map is repeated at its edge, and both stay 1. To keep a picture's border use a fill, a map (`mask("name")`), or the picture `fit` to a box larger than the canvas by the reach of the blur.** A canvas mask is drawn on a plane larger than the canvas by the reach of its blur (and of the blurs of the masks that nest in it), so a shape that crosses the canvas edge feathers as on a larger canvas; a shape that ends at the canvas edge is 0.5 there. A fill with no `fit` and no `place` covers the margin, so a feathered fill mask stays 1 to the edge; a fill that is fitted or placed is the shape `fit` and `place` make of the canvas box and ends where it ends, so a layer is the same shape whatever its blur adds. A map is blurred on its own raster with its edge replicated (a matte that touches the picture's edge goes on past it): on the look path the raster is the part of the picture that shows, so a `cover` cut replicates at the cut | |
| Text | `Layer.text(content, font=, color=, **options)`: a bundled font by name or a `Path` to a font file, read at the call (the box needs the font, so a font file is not read at `render` as pictures and maps are). Vector's `text` options pass as plain data and vector's reader checks them at the call, naming the option; `anchor`, `inside`, `runs`, expressions and Params are `InvalidError`. The box is the glyphs' bounds, and its origin the text's anchor, so `place(dx, dy)` puts the first baseline's left end, middle or right end (by `align`) at `(dx, dy)`; with `fit` the box is what is fitted. A text with no glyph (empty, or only spaces) is `InvalidError` naming `content`. The text is vector's own picture of the text and equals vector's drawing of the same text with the anchor there, at whole and at fractional anchors and output scales: measured, at most 0.05% of the premultiplied values differ by more than one level of 255 and none by more than 1.25 levels (the pictures are 8-bit) | |
| Depth and count | One rule (`budget.rs`), kept in the `Scene` as layers are added and checked at `add` and at `mask`, never at render. **Depth:** `MAX_DEPTH` is 16: how deep drawing leans on other layers, each mask layer a level (a layer's mask layer, its mask layers' mask layers...) and each reference a level of its chain (a reference to a reference to a layer is two); a reference used as a mask layer is a level more than the layer it names (the chain and the nest add: `masks + links + 2` for a stack layer masked by a nest of `masks + 1` mask layers whose innermost is a reference at the end of `links` references). A `mask` call refuses a nest past 16 (masks only: a reference's chain is not known until the scene finds it) and `add` refuses the whole, naming `layers`. **Count:** `MAX_LAYERS` is 1,000: the layers one render draws, each stack layer, each mask layer and each reference counted at each use. **A change from C3, which counted the stack and the mask layers of one layer apart:** a scene of 500 layers each masked by one shared mask layer draws 1,000 (accepted) and one of 501 draws 1,002 (refused naming `layers`), though a scene may hold 1,000; a layer with its mask layers draws at most 1,000, refused at the `mask` call. A reference counts once, as a layer, and the layer it names counts nothing for it (it is drawn again, not as another layer's mask or opacity). **Scenes as layers (C4c) count in the same budget**, each use: a group is one layer plus the layers its scene draws (the scene's count, kept in the `Scene` as layers are added, so `Layer.scene(inner)` costs a constant and `Scene::add` checks the sum), and one level deeper than its scene (its depth, likewise stored): a scene of 600 layers used twice is 1,202 (refused naming `layers`), a nest of 16 scenes is accepted and 17 refused at `add`; a group used as a mask layer is two levels (the group's, then the mask's); a reference to a group counts the group again. The depth is the number of calls the recursion nests, so the one bound serves the stack: masks, scenes and references mixed to 16 deep (6 links, 4 scenes with a reference to the first, 3 scenes as mask layers: 6 + 4 + 2 x 3) are drawn, printed, copied and dropped in a thread of 256 KB in the test profile, an unoptimised build and a release build; the unoptimised build overflows between 64 and 128 KB (so 256 has a margin of two) and the release build runs at 64. `Debug` of a group prints its canvas, count and depth, not the nest. Drawing, `Debug` and dropping a nest of 16 deep take no more than 64 KB of stack in an unoptimised build (32 KB overflows) and are tested in a thread of 256 KB, with a chain of 16 references and a mix of masks and references to 16 (an unoptimised and an optimised build): the walks over references are iterative and a reference holds a name, not a layer, so a chain nests nothing. A mask's `Debug` shows its settings and its depth, not the layers it nests | |
| Drawings | A drawing is drawn to the pixel: vector draws a frame at one scale, centred, in a raster whose short side is rounded up, which left a quarter to a half pixel of slack (a translucent border: alpha 0.749 at 1,920 by 1,080 drawn at 1,000) and could not draw a frame at two scales (`stretch` letterboxed). The adapter instead scales the drawing's shapes to the density the layer is drawn at along each axis (a factor of about 1.00x; the two densities of `stretch`), moves them by the fraction of a pixel the box starts at, and gives vector a frame that is exactly the raster at one scale. The warp then copies the raster onto whole pixels (a move or a zoom with nothing turned, skewed or flipped) and does not cut the drawing's own antialiased edge again. A full-frame drawing is opaque to the edge at any size, `stretch` stretches, and a text is where vector puts it at fractional anchors and scales. **Limits:** a stroke's width, a shadow's offset and a halftone's pitch are in design pixels at the mean of the two densities, so under `stretch` they are not stretched (exact scale, up to the rounding ratio, otherwise); a gradient, a clip or a mask of the drawing is in the drawing's own frame and is not scaled with its shapes, so under `stretch` it does not stretch; a frame within a pixel of vector's 100,000 limit is refused when the layer is drawn at a density below 1 |  |
| Parameters (C6a) | One list per layer, written in `parameters.rs` (the address grammar: its table) and read by `with_values` beside it. **A value is set only at an address the layer lists**: any other address is refused, with the addresses of the layer (or the layers) and the nearest by the blend rule's own search (`suggest.rs`; a word of over 200 characters gets no search, a list of over 40 addresses is cut). The settings of one layer are given to it together; those of one look go to image's `Recipe::with_values` as a set (its cross-field rules hold for the set, its words are the message), those of one mask layer to that layer, those under a group to the inner scene's `with_values`. `with_values` sets the values on the layers **as they were added** and builds the scene again through `add`, so references are found again and the depth and the layer count are checked again; a layer nothing was set on shares its source with the old scene. Nothing is set unless every value is accepted (the original scene is never touched). Measured (release build, arm64 Mac): 1,000 fills, one value 0.24 to 0.34 ms, a value for each layer 1.7 ms (building the same scene with `add` 0.42 ms); `parameters()` of them 2.1 ms; a ten-value frame (two looks, a drawing's two Params, a scale, four opacities) of a mill-sized scene (a 1,920 by 1,080 photo with two maps, three looked references, a drawing, four words; 209 parameters) 72 microseconds, against 156 ms for the render of that frame at 720 | |
| Parameter limits (C6a) | **(1)** `default` of a `text`, a `color` and a look's number is the value the scene holds: image's `Number` has no default and the value a layer was built with is not kept. **(2)** A layer under `perspective` lists no placement field (its corners are not a parameter, and `dx` would replace them). **(3)** A mask by one of the layer's maps has no mask layer, so no field (its `feather` and `invert` are not parameters); it still counts in the numbering (`mask`, `mask#2`...). **(4)** A mask layer lists no `blend` and no `light` (a mask is not joined to what is below). **(5)** A vector Param is a number; a list of numbers (one per shape) is not a value. Vector's plain-number unit (`""`) is written `count`, because a `float` of the format needs a unit. **(6)** Image lists the numbers of a frame for any recipe; a look has no frame, so `frame.*` is not listed. **(7)** A colour is written in lower case; a colour with an alpha (`#rrggbbaa`) is accepted and listed, but the format's `color` pattern is six digits, so such a parameter does not validate against the schema (a Need). **(8)** `rotate` is stored wrapped into 0 to 360 (`place` wraps it too), so its value reads back as the turn, never 370. **(9)** `value` of a held picture or map is a handle (`Held`) that shares the pixels; `numpy.asarray` makes the array | |
| Saved scene (C6b) | **Where it differs from the contract written first:** (1) a picture or a map that is a hole is `null` (a template saves); (2) a drawing's set Params are `values` beside it (vector keeps them outside its document, so the document alone would lose them); (3) a mask is an object of `layer` or `map` and its options, in call order; (4) `to_json()` takes no folder: a path is written as the scene holds it (a scene read with `base` keeps the relative path it was read with, one made by calls the path it was given), so to move a project give relative paths and read with `base`; (5) a font file is read at load, not at `render` (`Layer::text` needs its box), and a look file or a drawing file named by a path is read at load; (6) `layers` may be left out of a scene (none). **Limits:** a document nests at most 128 levels (`serde_json`'s bound), so a vector document that nests deeper than 125 inside a layer is refused on reading (vector writes up to 128); a file name that is not UTF-8 is `InvalidError` at `to_json`; equality hashes the pixels of a held picture (above); no limit on the size of the text (`MAX_LAYERS` bounds what is built). `float_roundtrip` is on for `serde_json` (its default parse of a decimal is within 1 ulp, and a placement must read back as the number written: a proptest found it). **Measured** (release test build, arm64 Mac, 1,000 fills each at its own place, 50 KB of text): `to_json` 0.87 ms, `from_json` 1.5 ms, `==` 1.3 ms; a mill-style scene (three files, six looks, two drawings, 4 KB; the extension in a development build) `to_json` 0.13 ms and `from_json` 29 ms, as long as building the scene by calls (28 ms: image checks each look) |
| Cache (C7) | **What is kept** (`cache.rs`: one bounded LRU in the `Scene` (a banded render spares what it has used: a scan of more than the bound keeps what fits; a scene drawn again that the cache cannot hold keeps all but two layers), shared by every scene made from it by `add` and `with_values`, found by a SHA-256 key; `==`, `to_json` and `parameters()` never see it; safe to render from several threads): (1) a layer's raster **before the warp**: its source drawn at the drawn density, its looks and own-map masks run (not the fused picture with no look and no map mask: it hands over its own pixels); a layer that only moves, turns, flips, skews, changes `aspect`, `opacity`, `blend` or its masks is warped again from it, and a reference whose extra place leaves the density shares the named layer's raster; (1b) **each stage of a chain of looks** (a layer with two or more looks: its own, or the named layer's followed by a reference's): the raster after each look but the last, with the maps it needs (at its size, grown with it) and the room it was grown by, found by the key of everything below the looks (the source, `light` and the request, as in (1)), the room the whole chain grows the raster by, and the looks up to that stage; changing look `k` runs the looks from `k` only, a reference that adds looks to a chain resumes from the stage any chain with the same first looks left (the key is the same), and a stage that lacks a map the resuming chain reads (a mask by a map no earlier chain used) is not resumed from; the last look's raster is the layer's finished raster of (1), so a chain of `n` looks keeps up to `n - 1` more rasters of its layer's size (a one-look layer, none), counted in the same 2 GiB, with the stage's maps counted in each stage though they are shared; a stage is **expendable**: it is kept only where it fits beside the other entries by making other stages leave, and it is the first to leave when any other entry needs room, so a stage never costs the cache a finished raster (the rest leave least recently used first, by bytes); (2) a group's isolated raster (its inner layers are keyed by what they are made of, so a group at a new phase draws its raster again and not the layers inside it); (3) the plane of a mask layer, so a mask layer shared by several layers, or drawn again by a reference, is drawn once for a target; (3b) the join after every other layer of a whole render of up to 2^22 pixels (expendable: kept only where it fits beside the finished values), so one layer changing lays that layer, those over it and at most one under it; (4) a picture or a map read from a file, by path, size and modification time. **What is in the key**: the source (a held picture or map by the digest of its pixels, made once; a file by path, size, modification time and the digest it was saved with; a drawing by the digest of its document, its Param values, its box and, for a text in a font file, the font's bytes; a fill by its colour; a group by its inner canvas and each inner layer by the same list; a plugin layer by name and options), the looks (the digest of each recipe's document, in order), the masks by a map (name, feather, invert), `light`, and the request: the part of the box, the density per axis, the fit factor, the canvas handed to image (it sets a look's design scale) and, for a drawing, a text and a group only, the pixel phase and the pixels the scenes round it hold. A mask plane adds the mask layer (placement, masks, opacity, each as for a layer), the reading, the feather, the inversion and the target (canvas, margin, offset). Plain values are written by their `Debug` text, so a field added to one is in the key without anyone adding it. **The bound** is 2 GiB of pixels, planes and decoded files (`cache::MAX_BYTES`), the least recently used leaving first; an entry larger than the bound is not kept and nothing is refused ("The cache" in [design-decisions.md](design-decisions.md)). **Limits:** (1) a file is keyed by path, size and modification time, so a change that keeps all three is not seen (a scene saved with digests checks them when the file is read, and a hit skips that read); (2) a file a look's recipe names (a table or a picture given as a path) is keyed by its path in the recipe, not by its content; (3) a render takes the layers in turn, so a poster whose reusable layers together are larger than the bound reuses none of them (the least recently used is always the next one wanted): a 15-layer A3 poster needs about 1.2 GiB of the 2; (4) the first render of a layer with a look hashes its held picture (22 ms for 3 megapixels, 0.1 s for 12, on eight cores), once; (5) a stage is kept only for a chain of two or more looks, and the named layer of a reference keeps its finished raster, not a stage: the first frame of a reference that adds looks to a one-look layer runs that look once more, and a chain whose maps differ from those a stage was kept with (neither a subset of the other) draws the stage again each time the two alternate; (6) a plugin kind that reads something besides its name, options, rectangle and scale gets a stale picture: it must put the input in its options |

**Looks live on the layer** (C5a; what the C1 review found): looks and the masks by a picture's own maps are a `Finish` of the layer body
(`finish.rs`), run by `Opened::draw` on whatever the source drew, so a source only draws, at the size a look runs at, and hands
over the maps its looks and masks use. Image is still named in one file, `source/picture.rs`, which holds the picture and the
`Look`. The `Source` trait and the `Layer` fields are `pub(crate)`; a plugin layer is the public trait `LayerKind` (C7), wrapped as a source. The trait is
"draw this part of your box at this scale" and answers with the raster and the part of the box it shows, so a fill without a
look answers with one pixel and a picture without a look with its own pixels.

**Room, as built (C5c).** `Drawn` carries `room`, the pixels its raster reaches past the window; `Layout::warp` clips the window grown by it, and `Finish::run` sets `ends_at_window` where the grown raster ends where the window did.
`Layout` held one `Placed`: C4b gives it a list of extras (the extra `place` of `Layer.of`). `Source` has no identity, which
`Source::keyed` writes it (C7).

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
| C3 regression, the mask-free path (five runs of each build, interleaved, on a quiet machine, a release build with the pinned toolchain): 1,000 fills on 1,024 by 1,024, 0.660 to 0.693 s before C3 (`0904c09`) and 0.660 to 0.674 s at the end of the review. Timings move with the machine's load: the same two builds read 0.72 to 0.80 s with a load average of 8, together, so a comparison is made interleaved or not at all | 19 MB both; one fill of 2^27 pixels, 0.32 to 0.33 s then 0.33 s, 2.15 GB both |
| C3 regression, pictures: the 12 megapixel picture unplaced 0.024 s, moved 0.030 s, scaled 0.9 and moved by half a pixel 0.127 s, turned 17 degrees and scaled 0.9 0.447 s, skewed 89 degrees 0.176 s, in perspective 0.52 s, cut by `cover` into 720 by 1,280 0.023 s: each within 2% of the build before C3 | 386 MB each (523 MB scaled; 306 MB cut), the same to 0.1 MB |
| C3: a fill masked by a fill, feathered, on a 2,400 by 3,200 poster (3,200 pixels on the long side): feather 3 design pixels 0.16 s, 30 0.22 s, 300 0.80 s | 190, 190 and 240 MB |
| C3: a nest of 15 mask layers each feathered by 30 design pixels on that poster, 8.6 s (without a feather, 0.13 s) | 630 MB (186 MB with none; each level's plane is larger than the one it serves by the reach of its blur) |
| C4a regression, the blend-free path (ten runs of each build, interleaved, a release build): 1,000 fills on 1,024 by 1,024 0.663 to 0.676 s before C4a (`54c045c`) and 0.663 to 0.674 s after (five runs of each read 0.8 to 1.5 s while other work ran on the machine, both builds alike: compare interleaved or not at all); one fill of 2^27 pixels, six interleaved runs of each at a load average of 5 to 7: 0.299 to 0.577 s before and 0.294 to 0.700 s after (the least of each, 0.299 and 0.294 s, is the comparison: no change) | 19 MB and 2.15 GB both |
| C4a regression, pictures: two 12 megapixel pictures, the upper one unplaced 0.052 s before and 0.056 s after (median of five), moved 0.057 and 0.053 s, scaled 0.9 and moved by half a pixel 0.200 and 0.192 s, turned 17 degrees and scaled 0.9 0.577 and 0.579 s | 578 MB (907 MB scaled), the same |
| C4a: a 12 megapixel picture of alpha 0.8 over another, unplaced, by each mode (least of three runs, load average 2 to 8, release build), on smooth pictures (gradients) and on noise (an independent random value per channel): normal 0.033 s smooth, 0.037 noise; multiply 0.104, 0.103; screen 0.102, 0.102; darken 0.100, 0.102; lighten 0.104, 0.102; color-dodge 0.106, 0.107; color-burn 0.109, 0.105; difference 0.103, 0.101; exclusion 0.105, 0.106; **overlay 0.107, 0.252; hard-light 0.105, 0.245; soft-light 0.113, 0.251; hue 0.163, 0.313; saturation 0.141, 0.312; color 0.077, 0.152; luminosity 0.079, 0.152**; a light layer by screen 0.106, 0.115. **The cost of a blend depends on the data:** the modes that choose a formula by the value (overlay, hard-light, soft-light) or sort the channels (hue, saturation) take about 2.5 times as long where the choice changes from pixel to pixel (noise) as where it does not (a gradient). A blended layer costs three to ten times a normal one, which copies its pixels. **Blended fills:** 1,000 `multiply` fills of alpha 0.5 on 1,024 by 1,024 take 11.4 to 12.0 s (7.9 s on a quieter machine, by the review) against 0.66 s for 1,000 `normal` ones: about 8 to 11 ns for each pixel of each layer against 0.66 ns, for the three divisions that give the backdrop its straight colour, the mode and the clamps, where normal is one multiply-add a channel | 578 MB, the same as with normal |
| C4b regression (release build, five runs of each build interleaved, load average 4.5 to 5.7, the least and the median of each): 1,000 fills on 1,024 by 1,024 0.6635 and 0.6639 s before (`f0cb480`), 0.6632 and 0.6648 s after; two 12 megapixel pictures, the upper one unplaced 0.0332 and 0.0338 before, 0.0339 and 0.0341 after; moved 0.0422 and 0.0426, 0.0432 and 0.0434; scaled 0.9 and moved by half a pixel 0.1494 and 0.1500, 0.1503 and 0.1507; turned 17 degrees and scaled 0.9 0.4501 and 0.4507, 0.4513 and 0.4521: no change beyond 2% (the body and join split and the extras are on the path of every layer; the layer without extras runs the code it ran, bit for bit: 45 scenes hashed before and after) | not measured again |
| C4b, the cost of a reference (the same build, 12 megapixel pictures, five runs, median): **a reference draws the named layer's body again**, at the density the composite gives, so it costs what a second copy of the layer costs, and the cost of a second picture of its own: an unplaced picture and a copy turned 17 degrees and scaled 0.9 0.464 s, the same with a reference to the picture given that extra place 0.453 s; a copy scaled 0.9 0.105 s, a reference with an extra scale of 0.9 0.099 s. The peak was not measured per scene (the four renders of the run together peaked at 907 MB). **What C7 may share:** a reference whose extra place leaves the density unchanged (a move, a turn, a flip, a skew; its `scale` 1) draws the body at the density the named layer was drawn at, so the named layer's drawn raster is the one it needs and C7 may draw it once and warp it twice; a reference whose extra place has a `scale` draws (and runs the inherited looks) at the composite density, a raster of its own that nothing shares: the halo follows the scale, so it is a different picture. The shadow and the reflection of the examples are of the first kind | |
| C4c regression (release build, five runs of each build interleaved, load average 4.9 to 5.1, the least and the median of each): 1,000 fills on 1,024 by 1,024 0.6643 and 0.6650 s before (`256c523`), 0.6644 and 0.6646 s after; one fill of 2^27 pixels 0.3272 and 0.3305, 0.3287 and 0.3298; two 12 megapixel pictures, the upper one unplaced 0.0351 and 0.0354, 0.0351 and 0.0362; moved 0.0412 and 0.0414, 0.0408 and 0.0411; scaled 0.9 and moved by half a pixel 0.1487 and 0.1498, 0.1475 and 0.1509; turned 17 degrees and scaled 0.9 0.4561 and 0.4567, 0.4580 and 0.4585: no change (the least of each within 1%, the medians within 2%, the other way as often; the draw entry is shared and the layers carry one more field; 14 renders of the examples hash the same before and after the refactor) | not measured again |
| C5a regression (release build, five runs of each build interleaved, load average 5 to 7, the least and the median of each): 1,000 fills on 1,024 by 1,024 0.6672 and 0.6680 s before (`7d9a18c`), 0.6679 and 0.6688 s after; one fill of 2^27 pixels 0.3288 and 0.3478, 0.3182 and 0.3324; two 12 megapixel pictures, the upper one unplaced 0.0422 and 0.0445, 0.0434 and 0.0456; moved 0.0464 and 0.0520, 0.0454 and 0.0516; scaled 0.9 and moved by half a pixel 0.1627 and 0.1658, 0.1606 and 0.1628; turned 17 degrees and scaled 0.9 0.4678 and 0.4704, 0.4664 and 0.4671; a 3 megapixel picture with a `grade` look 0.0442 and 0.0449, 0.0415 and 0.0454: no change (every least within 2%, the other way as often; a layer with no look takes the path it took, and 45 scenes hash the same before and after) | not measured again |
| C4c, the cost of a group (the same build, a 12 megapixel picture inside a scene used as a layer over a 12 megapixel picture, five runs, median): unplaced 0.070 s (a plain unplaced picture over it 0.035 s: the group is drawn into its own buffer, divided to straight alpha, multiplied again, then joined); placed (turned 17 degrees, scaled 0.9) 0.269 s (plain 0.457 s: the picture is resampled once at the scale 0.9 into the group, and the turn alone is the cheaper warp; not a saving of the group, the two layers do different work) | |
| C4c, the peak of a nest of groups (a 12 megapixel picture at the bottom of a nest of single-layer scenes, 4,000 by 3,000 design pixels at size 4,000, release, peak resident set of the whole render): no group 387 MB, 1 to 4 groups 579 MB each (a group's buffer is allocated and then touched only when its first layer is drawn, and an inner group is drawn first, so the buffers wait untouched and the peak does not grow); the same nest with a layer under each group (so that each level's buffer is touched before the next is drawn) 579, 771, 963 and 1,155 MB: 192 MB a level, which is 16 bytes of the 12 megapixel picture a group holds, so the bound of 2^27 pixels in sum (2 GiB) holds the peak of the groups to 2 GiB over the output picture and the layer being drawn | |
| C7 regression (release build, the cases above drawn cold each run, five runs of each build interleaved, load average 3.1 to 4.4, the least and the median of each): 1,000 fills on 1,024 by 1,024 0.6646 and 0.6652 s before (`5d1ea9d`), 0.6646 and 0.6676 s after; one fill of 2^27 pixels 0.3284 and 0.3318, 0.3278 and 0.3329; two 12 megapixel pictures, the upper one unplaced 0.0347 and 0.0351, 0.0351 and 0.0356; moved 0.0408 and 0.0417, 0.0416 and 0.0421; scaled 0.9 and moved by half a pixel 0.1490 and 0.1498, 0.1472 and 0.1492; turned 17 degrees and scaled 0.9 0.4595 and 0.4617, 0.4665 and 0.4678: no change (the fused picture path is not cached and not hashed; the least within 2%, the other way as often). **One cost:** a 3 megapixel picture with a `grade` look 0.0413 s before and 0.0633 s after, a first render: the held picture is hashed once (48 MB, 22 ms) | not measured again |
| C7, the loops (release build, load average 2.4 to 3.1, 12 frames each, the same scene drawn by a scene made afresh each frame and by one scene, `==` bit for bit): **the wow poster's loop** (`poster.build` with a synthetic 3,000 by 4,000 photo, the dog's bob and sway, the burst's pulse and the sun's swell of `loop.py`, 1,080 by 1,620): 0.95 s a frame with a scene made afresh each frame (re-measured 2026-11-16 with demo 09's own builder; 0.506 s on the machine of 2026-11-14), **0.19 s** with the cache (the first frame 0.59 s: it hashes the photo, the keyline's host and the matte, 0.17 s); what is left is the burst, drawn each frame because its Param is the pulse, the sun at its new scale, and the join of 24 layers. **A mill frame** (`mill.build`, a 3,000 by 2,000 photo, all the values of `frame_values` changing, 720 by 1,280): 0.524 s before, **0.413 s** on the machine of 2026-11-14 (re-measured 2026-11-16 with demo 10's scene and photo: 0.30 s a frame at every commit, the same pixels): the frame redoes the night photo's two references (their swell changes, so the look chain with the night grade under it runs again, 2 x 0.2 s) and takes the rest (the photo, the logo, the glows, the words) from the cache. With only one value changed: all values the same 0.025 s (20 times), a word's opacity 0.025 s, the logo's ring 0.030 s, the logo's glow 0.030 s, the swell of the sign 0.196 s, the swell of the windows 0.238 s. **The phase costs little here:** with every other value held, the logo moved by whole pixels takes 0.025 s a frame and by a fraction of a pixel (its drawing and the glow's drawing and look redone) 0.029 s; in the wow loop the dog and its keyline are pictures, which have no phase in their key, so their bob and sway cost nothing extra | |
| B1, a 15-layer A3 poster at 300 dpi (4,961 by 3,508, drawn 1:1; a stand-in for C7's poster, whose builder was not kept: a photo covering the canvas, a rotated one, a looked one, one masked by its own feathered map, a multiplied band, a drawing, two texts, a group at 2x and 10 degrees, a looked reference, a feathered mask layer, a looked light fill, a picture in perspective, an overlay; release build, load average 5 to 20): whole 1.66 s the first render, 0.55 s the second; banded in 1,000 rows 1.87 s; **peak 1.47 GB whole against 0.89 GB banded** (the inputs, about 0.2 GB, included; the band is 79 MB, the rest is the looked layers and the cache). `render` on `main` and on this branch, three runs of each interleaved: 1.72 to 2.87 s and 1.68 to 2.26 s, 1.47 GB both. **A0 at 300 dpi** (9,933 by 14,043 pixels, 139 megapixels, eight layers with a group of the canvas's size, two photos, a rotated one, type at 1,600, a feathered mask layer) by `render_to_png`: 25 s, **peak 1.77 GB**, a 44 MB PNG; `render` refuses it by the 2^27 pixels | |
| P2, a 13-layer poster of 1,200 by 800 pixels drawn at 1,200 on the long side (pictures, one shrunk past 2x, looks, a mask, a perspective layer, a light layer, type, a group, blends; release build, Apple M4, load average 3 to 7, median of 15, each case in its own process, the picture bit for bit the same before and after): drawn cold 164 ms before, 179 ms after (keeping every other join costs the first render about 9%; keeping each cost about 27%); the same scene again 68 ms before, 2 ms after; a top layer's opacity changed 70 ms before, 4 ms after; the bottom layer's opacity changed 65 ms before, 20 ms after (every layer is laid again, now on all 10 cores); the A3 poster of the same layers (4,960 by 3,307) in bands of 512 rows 2.4 to 2.6 s before, 1.6 to 1.8 s after (the bands are not kept, only laid on all cores) | each kept join is 15 MB here (16 bytes a pixel, expendable, up to 2^22 pixels a render) |
| Look chain, stage by stage (optimised test build, load average 3 to 5, five runs of each mode interleaved, medians of 9 frames, stages switched off for the baseline; a synthetic 3,000 by 2,000 photo with two maps, image's own night and swell recipes of `mill.py`, 720 by 1,280): **a mill-style frame** (two references, each the night look then a swell, every value changing): 1.03 s before, **0.515 s** (the night look is 0.26 s and a swell 0.26 s here; the frame is left with the two swells); changing only the swells gives the same, the words cost nothing. **Single-look layers are unchanged** (cold night photo 0.28 to 0.31 s against 0.29 to 0.31; light layer with room 0.66 to 0.71 against 0.66 to 0.69; a one-look loop 0.34 against 0.34; a looked group 0.145 against 0.145). **Memory at the bound** (five 4,000 by 3,000 layers of three looks each, 2.9 GB of stages and finished rasters against the 2 GiB bound; peak resident memory of the process): 2.5 GB without stages, 3.5 to 3.7 GB with them (the cache holds up to its bound); changing the look of the last layer 4.1 s before, **0.3 to 0.6 s**; changing the first look of a layer 3.9 s before and 4.1 to 5.9 s now (it runs the whole chain, as before, and keeps the stages again). With stages ordinary entries, the same scene took 25.8 s for the last layer (the stages evicted the finished rasters of the scan), the reason stages are expendable |
| C7, a 15-layer A3 poster at 300 dpi (3,508 by 4,961 design pixels, drawn 1:1; a picture with a look and a map, a drawing of 36 bars, five texts, a reference with a grain look and a feathered mask, a group and a reference to it at half size, two feathered masks, a rule, a vignette, grain; whole, release build, load average 6 to 7; the builder was not kept, so these are the original figures, not repeated: the rebuild is the next row): first render 5.2 s before and 5.2 to 5.4 s after; **peak 2.16 GB before and 3.14 to 3.5 GB after** (the inputs, 0.6 GB, included: the cache holds what the render would have freed); the same poster with one headline changed: 5.0 s before, **0.39 s** after, peak 3.15 GB. **The bound:** the reusable layers of this poster are about 1.2 GiB: bounds of 1,100 and 1,200 MiB give 5.0 and 5.2 s (a render takes the layers in turn, so least recently used evicts what is wanted next), 1,280 MiB and up 0.39 s; 2 GiB is chosen (a third more than it needs). **The cache at its bound:** 12 looked fills at A3 (3.3 GB to keep): peak 3.7 GB against 1.6 GB, second render 9.0 s against 8.7 s (none of it is reused: the cache pays 2 GiB for nothing when a render scans more than it holds) | |
| C7/B1, the poster rebuilt at the repeated-scan policy (2026-11-16; release build, one process a run, load average 2.5 to 4.4, no other heavy job, medians of three runs interleaved at `c2f26b8`, before the policy, and `8a32b28`, after it; a rebuild from the description above, not C7's builder, which was not kept: 3,508 by 4,961, a looked picture of 1,754 by 2,480 with a map, a drawing of 36 bars, five texts, a looked reference with a feathered mask, a group and a reference to it at half size, a rule, a vignette, grain, a looked reference: 15 layers): **in bound:** first render 2.86 s before and 2.89 s after, one headline moved 0.28 s at both, banded in 1,000 rows 1.15 and 1.22 s (spread 1.13 to 1.3), peak 2.58 GB at both: the policy changes nothing there. **Over the bound** (the same poster and seven more looked references of the photo, 22 layers; the layers exceed the cache because the second render at `c2f26b8` takes as long as the first): first render 6.44 s before and 6.30 s after, **one headline moved 7.25 s before and 5.81 s after**, a repeat render 7.19 and 5.87 s, banded 16.2 and 14.5 s, peak 3.44 and 3.46 GB. The pixels are the same at both commits in every phase (a hash of the float bits: in bound first `fd88f205282bd99a`, moved `7f8077c5fbc80cc0`; over the bound first `39da821ce1eb1865`, moved `be39289a666fb6b0`), and a banded render equals the whole one. |

`MAX_LAYERS` (1,000, now the layers a render draws, "Depth and count") and the 2^27 pixels stay. A mask is one `f32` plane, 4 bytes a pixel, and a layer with masks holds the product of its masks and, one at a time down a nest, the planes of the masks being drawn. The 2^27 applies to the output and to every layer's drawn size (its box at
the scale it is drawn at, `ceil(width x scale)` by `ceil(height x scale)`, the scale being `output scale x fit factor x
scale`: a layer that is large on a small canvas is `InvalidError` naming the layer and the size, before it is drawn). A large
`scale` on a layer that is looked, a drawing or a plugin kind meets it even where only a part of the layer shows; **a picture with no look and a flat colour draw only their own pixels and are limited to a side of 2^31 pixels instead** (B1), and a group is limited by the rows it draws on a band. A render holds the picture, the one layer being drawn and its working copies, not one picture per layer; **since C7 the
cache adds up to 2 GiB to that** (what a render reuses is kept: a 15-layer A3 poster peaks at 3.15 GB against 2.16 GB; a
render that scans more than the bound, 3.7 GB against 1.6 GB), and every peak in the table above was measured before C7. A picture at the limit takes 5.5 GB; there is no promise
that a machine has it, and `render_bands`, `render_to_png` and `render_to_pdf` (B1) lift it for the output: a band is at most 2^27 pixels and a picture is at most 65,000 on a side. Masks nest, references chain and scenes nest, to `MAX_DEPTH`.
**Groups: the sum.** A group's picture (the raster its scene is drawn on, `ceil(width x scale)` by `ceil(height x scale)` for the box it
is drawn for, premultiplied `f32`) is alive while the groups inside it are drawn, so 2^27 pixels bounds the **sum of the pictures of the
groups drawn inside one another** (the output picture and a layer's own raster keep their own 2^27, as before; a mask's plane is not in the sum).
Each group checks `outside + its own` before it allocates (`Request::held`, `Target::held`), and a nest over the bound is
`InvalidError` at `render` naming `size`, the pixels held and the bound: two nested groups of 2^26 pixels and a little more are refused when the inner
one is about to be drawn (tested without allocating a gigabyte), the same nest at a smaller size renders; a group alone is bound by the layer's 2^27 as a picture is.

## Examples

Each program is a `build(...)` function that returns a `Scene` from its inputs (a photo array, a matte, a path), and a
`__main__` block that loads real files and renders. A test calls `build` with synthetic inputs and needs no model.

| Program | Shows |
|---|---|
| [c1_first_picture.py](examples/c1_first_picture.py) | C1 (runs now: `uv run poe look`): a synthetic photo with one image look and a vector headline over it, rendered at two sizes |
| [c2_prints.py](examples/c2_prints.py) | C2a (runs now: `uv run poe look`): a pile of synthetic snapshots, each `fit` (cover, contain, stretch) and placed (turned, skewed, mirrored, scaled), and one tall photo cut three ways into a 9:16 frame |
| [c2b_perspective.py](examples/c2b_perspective.py) | C2b (runs now: `uv run poe look`): a checker road receding to a horizon, where its checks are smaller than a pixel and fade to grey, and two billboards on quadrilaterals, one mirrored |
| [c3_clip.py](examples/c3_clip.py) | C3 (runs now: `uv run poe look`): a landscape clipped into the letters of a text layer, type behind a cut-out figure (an inverted mask by its matte), a glow, a shadow and a vignette with feathered masks |
| [c4a_blends.py](examples/c4a_blends.py) | C4a (runs now: `uv run poe look`): a contact sheet of the sixteen blend modes on the same two layers, each tile named by a text layer |
| [c4b_references.py](examples/c4b_references.py) | C4b (runs now: `uv run poe look`): a sun with a glow (a light reference with a look, written below the sun it names), a figure with a flattened, skewed shadow and a mirrored, faded reflection (references with a place and a mask) and a headline hidden where the figure is (a reference as a mask) |
| [c4c_groups.py](examples/c4c_groups.py) | C4c (runs now: `uv run poe look`): a badge scene used twice at two sizes, and two cards faded as one group and one by one |
| [c5a_looks_on_everything.py](examples/c5a_looks_on_everything.py) | C5a (runs now: `uv run poe look`): a headline, a flat fill and a badge scene, each plain and with an image look (gradient map and grain, a canvas weave, a duotone applied to the badge as one) |
| [c5c_room.py](examples/c5c_room.py) | C5c (runs now: `uv run poe look`): a sun, a figure and a neon word with their glow as a light reference, each on a layer as tight as its drawing and on a frame padded by hand: the same picture; the look's room is image's |
| [poster_two_ink.py](examples/poster_two_ink.py) | M1: a photo in two inks clipped into a polygon, type behind a cut-out subject, flat shapes, grain, three sizes |
| [c6a_covers.py](examples/c6a_covers.py) | C6a (runs now: `uv run poe look`): one cover template (a hole for the photo, an accent band, a title) set six times by `with_values` and drawn on a contact sheet |
| [cover_template.py](examples/cover_template.py), [.rs](../crates/core/tests/it/cover_template.rs) | C6a (`build()` and `frame` run from synthetic inputs, the Python one) and C6b (the Rust one is a test module, `crates/core/tests/it/cover_template.rs`, run by `saved.rs`: saved, read back, filled): one scene, a photo, a title and a colour set by `with_values`; six covers |
| [c6b_saved_scene.py](examples/c6b_saved_scene.py) | C6b (`saved_and_loaded` runs from synthetic inputs; `python docs/examples/c6b_saved_scene.py` with image installed writes the picture): a scene saved, its picture path made relative to the scene file's folder, read back with `base`, drawn: the same bits, the scenes equal |
| [frame_sequence.py](examples/frame_sequence.py) | C6a (runs from `build()` and `frame`): a headline sliding in over 24 frames, one value set per frame; the ground comes from the cache (C7) |
| [cache_probe.py](examples/cache_probe.py) | C7 (`Layer.make` and `register_layer`): a layer that did not change is not drawn again, observed through a counting plugin layer; the Rust test is `crates/core/tests/it/cache.rs` |
| [composed_world.py](examples/composed_world.py) | M2: a cut-out dog on a grid floor under a striped sun; shadow and reflection by reference, a glow on the sun only |
| [mill.py](examples/mill.py) | M3 (`build()` and `frame` run from synthetic inputs; the look's room is C5b): a night-graded photo, windows and sign lit as layers, a logo with its own glow, a 9:16 crop by `fit`, every moving value per frame |
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
| `Scene` | `Scene(width, height)` | an empty canvas of `width` by `height` design pixels: whole numbers, 1 to 100,000. A design pixel is what a length of the scene (`dx`, `fit`, `feather`) means; a look's `px` is another unit ("Looks", Design pixels) | `InvalidError` |
| `add` | `add(*layers)` | the scene with `layers` on top of its stack, first argument lowest | `InvalidError`: a name used twice (naming both layers); a `Layer.of` that names no layer of the scene or its own `add` call, or whose chain leads back to itself, also through masks (naming `<layer>.of` and the cycle); a look or a map mask of a reference that the layer it names cannot take; more than `MAX_LAYERS` (1,000) layers drawn (stack layers, mask layers, references, and the layers of each scene used as a layer, each use counted); drawing that leans on other layers (masks, references, scenes as layers) more than `MAX_DEPTH` (16) deep. `TypeError`: a non-`Layer` (wrap a scene with `Layer.scene`) |
| `layers` | property | the layers, bottom first, a tuple | |
| `width`, `height` | properties | the canvas | |
| `parameters` | `parameters()` | every parameter, by address, in layer order ("Values") | |
| `with_values` | `with_values(values)` | the scene with `values` (a mapping from address to value) set | `InvalidError`: an address the scene lacks (the message lists the valid ones and the nearest), a value outside its range or of the wrong kind (the address, the rule and the value) |
| `render` | `render(size)` | the picture ("Output") | `InvalidError`, `ReadError` |
| `to_json` | `to_json()` | the scene as JSON text, self-contained ("Saved scene") | `InvalidError`: a picture or a map that is an array (it has no file). `ReadError`: a file whose digest is to be written cannot be read |
| `Scene.from_json` | `Scene.from_json(text, *, base=None)` | the scene back; a relative path in it resolves against folder `base` (a `Path`; default: the working folder). Reads no picture or map: those are read at `render` (a font file, a look file and a drawing file named by a path are read now) | `FormatError`; `ReadError` for a font, a look file or a drawing file that cannot be read |

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
3. **Looks,** in call order, on the fitted raster (layer space), whatever the source. A look's `px` are image's design pixels, counted as
   the layer is fitted, before `place` ("Looks", Design pixels). A look that has a centre or an edge (a vignette, a warp) takes it from the fitted box. Looks see
   the layer's colour over black. **Room** (C5c): a light layer's raster grows by its looks' spread, so a halo or a bloom is
   not cut at the box's edge; the room is black over transparent, and stage 7 makes it light. A layer that is not light has no room.
4. **Own-map masks** (`mask("name")`): layer space; the map is cut and fitted with the picture.
5. **Place.** The finished raster is flipped, stretched along y by `aspect`, skewed and rotated about `anchor`, then moved by
   `dx`, `dy` (or `perspective`), by one warp. Its `scale` is in the drawing: the source drew at that scale and the look ran
   at that size, so a `scale` of 1.32 scales a halo with it.
6. **Layer masks** (`mask(Layer)`): canvas space, multiplied.
7. **Light** (`as_light`): the layer's alpha is made from the brightness of its colour.
8. **Opacity.**
9. **Blend** over what is below.

A `Layer.of` inherits stages 1 to 5 of the layer it names and owns stages 6 to 9 ("Reference").

**Stage 3 on every source** (built in C5a). A layer with no look is drawn as before: its source's raster, one warp. A layer with a
look draws its source at the drawn size, runs the look there, and puts the result down by one warp:

- a picture is resampled to the drawn window (its maps with it, those a look or a mask uses); a drawing, a text and a group are
  already drawn at that size (a group's raster is its finished, isolated picture); a fill is made that size, a flat raster, in
  place of its one pixel;
- image gets the raster's straight colour (RGB, no alpha); the source's alpha is kept as it is and put back after the look, so a
  transparent corner stays transparent and a half-transparent fill keeps its 0.5; a light layer's look sees the colour over
  black instead and its result is opaque, so that the light the look makes outside the shape reaches stage 7; the result is
  brought into 0 to 1, a value that is not a number refused;
- the raster ends where the source's did (`covers`, `ends_at_window`), except a light layer's, which grows by the room of its looks (C5c);
- stage 4 follows on the same raster; a reference runs its own looks after those of the layer it names.

**Stages 4 and 6 in the one-warp paths** (built in C3; every mask is a factor on the alpha, so a mask-free layer takes exactly
the path it did before):

- **Stage 4, a map's mask.** With no look the source hands over the picture's own pixels, and the map (same size as the picture,
  blurred by `feather` at the picture's resolution in design pixels of the fitted layer, then inverted) is multiplied into
  their alpha there: one copy of the picture, then the one warp cuts, fits and places both. With a look the picture is
  resampled to the size the look runs at and the map with it (the same window and kernel), the look runs, and the map,
  blurred at that size and inverted, is multiplied into the look's alpha after it. Where the picture is not resampled twice a no-op look and no look agree to the bit in alpha: a plain, cover, contain, move or
  skew layer at one output pixel to the design pixel, feather or not. Where it is (a turn, a perspective or a reduction: the
  no-look path resamples picture and map once, the look path twice) they differ by what two resamples of the map differ by,
  and a blur made at the picture's size on one path and at the drawn size on the other adds the two kernels. Measured on a
  map of 0 and 1 that changes every few pixels, in the largest difference of alpha: turned 17 degrees and scaled 0.9, 0.21;
  in perspective, 0.17; turned 30 degrees at half size, 0.22 (feathered by 2 over a map that is one edge, 0.019); a cover
  crop with a feather of 2 and of 6, 0.11 and 0.09; reduced by two with a feather of 2 across an edge, 0.022. The review
  measured up to 0.37 on a cover crop of its own: a feather replicates the edge of the part of the picture that shows, so
  it differs where a crop cuts through a map's detail. These are the stated bounds of `the_two_paths_of_stage_4_agree...`.
- **Stage 6, a layer mask.** The mask layer is drawn, read, blurred and inverted on a plane the size of the canvas plus its
  margin, and the product of a layer's masks enters the render as a factor at the pixel each warped pixel lands on, as opacity
  does (and, later, light): no canvas-sized raster of the layer itself is made for it. A mask layer is drawn by the same code
  as a layer of the scene: its own `fit`, `place`, `perspective`, looks and masks apply, at the scale of the output.

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
  sharpness; a banded render draws a drawing by window, which lifts that.
- **Footprint.** Where the warp shrinks the picture each output pixel is the average of what its footprint covers: an exact area
  average where the map keeps the raster's axes (a scale, a flip, a quarter turn), a lattice of taps over the footprint
  otherwise (a turn, a skew, a perspective), at any skew. Where the footprint is no larger than a pixel it is plain bilinear. Limit: the lattice is a box average, not an ideal low-pass, so a fine pattern at about two pixels (a checker floor at mid-distance) can leave a faint moire; a smoother kernel (elliptical) is the later improvement.
- **Cost.** An animated `scale` draws the source and runs its look again every frame. `dx`, `dy`, `rotate`, `skew`, `flip`
  and `aspect` only warp a raster that did not change: C7 keeps it, so a frame that only moves a looked layer draws it from the
  cache (a drawing, a text and a group at a new fractional place are drawn again: the pixel phase is in their key).
- **Zoom limit.** A large `scale` on a layer with a look, a drawing or a plugin kind meets the 2^27 drawn-size limit even where only
  a part shows. A picture with no look and a flat colour are resampled from their own pixels and are not limited by it (B1: a side of
  2^31 pixels); a group is drawn for the rows a band reads when the picture is drawn by bands.
- **Design scale (C5b-lite).** Looks take image's design pixels ("Looks"). The canvas handed to image is the canvas at `output scale x place.scale`
  (not the fit factor), so a halo still follows `scale`.
- **Fit.** The window `fit` shows is computed from the source's real size when the scene renders. `stretch` has a scale for
  each axis. The scale that binds (width or height) ends at the box exactly, so a layer fitted into a box of its own shape is
  not touched.

### Sources

| Source | Signature | Promise |
|---|---|---|
| picture | `Layer.picture(source=None, *, maps=None)` | `source`: an array (height, width, 3 or 4; `uint8`, `uint16` or floats 0..1; 4 channels are straight alpha), a `pathlib.Path` (read as `imageio.load_picture` reads: upright, sRGB, alpha kept), or `None`: a hole the caller fills with `with_values` before `render`. `maps`: the picture's named maps, each `None` (a hole), a float32 (height, width) array 0..1, or a `Path` to a grey image (read as `imageio.load_map` reads). A map is a parameter of kind `picture`, `<layer>.map.<name>`, set by `with_values` like the picture: a footage frame with its matte, or a template with a cut-out, is driven by `with_values`. A map of another size than its picture is `InvalidError` at render naming both sizes. A look sees the maps its recipe uses (`subject`, `depth`, a `given` region); the others serve `mask("name")`. A `str` source is `TypeError` (pass `Path(...)`). Its box is its pixels, one design pixel each, at the canvas origin. An array from `imageio.load` has no alpha (image puts it over white): give a transparent PNG as a `Path` or `imageio.load_picture` |
| drawing | `Layer.drawing(source, *, inputs=None)` | `source`: a vector `Drawing` (anything with `to_json()`), its JSON text (a `str`) or dict, or a `Path` to a drawing file. Its box is the drawing's frame (`size`) at the canvas origin: a drawing without a frame is refused. `inputs`: the bytes or `Path` of each font or map file the drawing names, by name (vector's `Drawing.from_json(inputs=)`); a bundled font needs none |
| text | `Layer.text(content, *, font, color="#000000", **text_options)` | scene owns `content`, `font` and `color`. `font`: a bundled font's name (a `str`) or a `Path` to a font file. `text_options` are `btvec.text`'s (`size`, `axes`, `tracking`, `leading`, `align`, `width`, `fit`, `features`) as plain data, validated by the linked vector crate at the call; scene restates none of vector's defaults; a whole number is one option however it is spelt (`84` and `84.0`). `width` and `fit` mean vector's: the width the text wraps or fits to, not a placement (`fit(...)` on the layer is the placement). `anchor`, `inside` and `runs` are refused, and so are expressions and Params: scene places the text, and per-letter numbers are a `Layer.drawing`. Its origin is the text's anchor, the left end, middle or right end of the first baseline by `align`: `place(dx=100, dy=420)` puts it there. Its box is the glyphs' bounds. The colour is flat: a gradient on type is vector's text in a `Layer.drawing` ("Recipes") |
| fill | `Layer.fill(color)` | a flat `"#rrggbb"` or `"#rrggbbaa"` over the canvas box; `fit` makes it a band, a mask makes it a shape. Gradients are vector's: a rectangle filled with `btvec.linear_gradient` |
| scene | `Layer.scene(inner)` | another scene as a group: its canvas is its box (design pixels, at the canvas origin like a picture); it is drawn isolated at the density it will have, then fits, places, masks, blends, fades and is made light as one picture; a look on it runs on its finished picture. `inner`'s parameters are addressed under the layer, `badge.scene.label.text` (C6). A `TypeError` for anything that is not a `Scene`; the scene counts in `MAX_LAYERS` and `MAX_DEPTH` at each use ("What is built", Groups and Depth and count) |
| reference | `Layer.of(name)` | see "Reference" |
| plugin | Rust: `Layer::make(Arc<dyn LayerKind>)`, or `LayerKinds::make(name, &options)` for a registered kind; Python: `Layer.make(kind, **options)` | a layer drawn by something outside the scene (C7, C8). `LayerKind` has `name()`, `options()` (plain data) and `draw(area, scale)`: the scene asks it for one rectangle of the canvas, `area = [x, y, width, height]` in design pixels, at `scale = [across, down]` output pixels per design pixel, and it returns a straight-alpha `Raster` of `ceil(width * across)` columns by `ceil(height * down)` rows (a product within 1e-12 of a whole number counts as that number), else `InvalidError` naming `make`, the kind and both sizes. Its box is the canvas, as a fill's is; `fit`, `place`, looks, masks, `blend` and `opacity` act on what it answers. **Its identity is its name and options**: the same four (name, options, rectangle, scale) must give the same raster, and the scene keeps what it drew and asks again only for another rectangle or scale. **By name (C8).** A `LayerKindFactory` builds the kind from its options (`create`); a `LayerKinds` registry holds factories by name (`^[a-z0-9][a-z0-9_.-]*$`, at most 64 characters, and not `picture`, `drawing`, `text`, `fill`, `scene`, `of` or `make`; registering a taken name replaces it only for the same `identity`, else `InvalidError` naming the kind). A layer made through a registry remembers kind and options: `to_json` writes `{"make": {"kind": "<name>", "options": {...}}}` and `from_json` reads it back through the registry (`Scene::from_json_with`; Python: the process-wide one), so equal scenes draw equal pictures. A layer given to `Layer::make` has no name in a registry and `to_json` refuses it (`InvalidError` naming `make`); `==` compares name and options either way. **Python.** `register_layer(kind, draw)` (or `@layer_kind(kind)`) registers `draw(x, y, width, height, scale_x, scale_y, **options)`, which returns the (rows, columns, 4) array of that rectangle at those scales: `float32`, straight alpha, 0 to 1, as `Layer.picture` takes one; `options` are the keyword arguments of `Layer.make` and must be plain JSON data (an object). One function, the Rust trait's `area` and per-axis `scale` unpacked; a kind that needs one scale reads `scale_x` (the two differ by less than one part in the canvas's long side). A scene refuses a function that raises (`InvalidError` naming the kind, the exception and its message), an array of another size than asked (naming both), a value outside 0 to 1 or a NaN (naming the first, by row and column), and an array of more than 2^27 pixels (refused before its pixels are read). The GIL is taken for each call and released around the rest of the render, so several renders can run in threads. `Layer.make` of a kind that is not registered is `InvalidError` listing the kinds and the nearest; `Scene.from_json` of one is `FormatError` at `/layers/N/make/kind`, and options the kind refuses are `FormatError` at `/layers/N/make/options`. Entry points: a package names a module in the group `bullterrier_scene.layers`; the first query imports the modules of the group in name order, under a lock, and one that fails leaves the registry as it was and raises `InvalidError` naming the entry point (the next query tries again) |

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
| `opacity` | `opacity(value)` | sets the layer's opacity to `value`, 0..1 (default 1). A group fades as one. At 0 the layer is not drawn where it joins (no blend mode can change what is below: it costs nothing, and its files and masks are not read), while a `Layer.of` that names it still draws its body: the way to define a layer only to be referred to | `InvalidError`: outside 0..1 |
| `blend` | `blend(mode)` | how the layer joins what is below, the W3C compositing modes by their CSS names: `"normal"` (default), `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`, `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`. Blended in sRGB values, as vector and a browser do. A look's own `blend_mode` (image's spelling: `soft_light`) is inside the look and stays image's | `InvalidError`: another word; `TypeError`: not a `str` |
| `as_light` | `as_light(on=True)` | makes the layer light, or not (default: not). A light layer's premultiplied colour is its look's colour over black, `c`, and its alpha is the largest of R, G, B of `c`; its straight colour is `c / alpha` (0 where alpha is 0). So a glow shows beyond the shape; black adds nothing, a `screen` of the layer over black gives `c` exactly, and white is opaque white. Masks scale it. A layer that is not light keeps its source's alpha, after its masks, and a halo or shadow its look adds outside the shape is cut | `TypeError`: not a `bool` |

### Masks

`mask(by, *, use="alpha", feather=0, invert=False)` multiplies the layer's alpha by a mask. Calling it again multiplies
another mask in.

| Argument | Meaning |
|---|---|
| `by` | a `Layer`: drawn on the canvas as its own place says (stage 6), read as `use` says; it may be `Layer.of("subject")`, which follows `subject`'s parameters. Or a `str` naming one of this layer's own `maps`: a map of its picture, cut and fitted with it (stage 4). A reference reads the `maps` of the layer it names |
| `use` | `"alpha"` or `"luminance"` (0.2126 R + 0.7152 G + 0.0722 B of the sRGB values, times the alpha: white keeps, black hides), for a `Layer`, read from the mask layer's colour as drawn (a `Layer.of` mask: the named layer's picture with the reference's own looks); a map is its own values |
| `feather` | the standard deviation, in design pixels, of a Gaussian blur of the mask's edge (0 to 1,000,000; 0 is hard) |
| `invert` | 1 minus the mask (after the feather): type that passes behind a subject is masked by the inverted subject |

A mask layer is owned by the layer it masks and is outside the scene's names: it is addressed `<layer>.mask.<field>`, the
second `mask#2` (the only thing that depends on the order of the calls; an error of a mask layer that cannot be drawn is
addressed under it: `photo.mask.map.subject`). A mask layer's alpha includes its own masks, and its looks apply to its
colour, which `use="luminance"` reads (a mask read as alpha does not run them: a look never changes alpha); its opacity multiplies the mask, before `invert` (opacity 0.25 inverted is 0.75 where the layer is drawn and 1 where it is not). A mask layer with a name, a
blend other than normal or `as_light(true)` is `InvalidError` at the call. A `Layer.of` mask has the fields of a reference only.
Errors: `InvalidError` for a name that is no map of the layer (at the call for a picture layer; at `add` for a reference),
a map that is a hole when the layer renders (naming the layer and the map), a `str` mask on a layer that has no picture,
`use="luminance"` with a `str` map, `use` another word, `feather` below 0, past 1,000,000 or not finite, a nest deeper than
`MAX_DEPTH`, and at render a feather whose plane would pass 2^27 pixels. What is decided where ("What is built", Masks,
Mask layers, Feather, Mask edge) is written there once.

### Looks

`look(recipe, px="design")` runs an image look on the layer. `recipe` is an image `Recipe` (anything with `to_dict()`), the dict it
writes, or a `Path` to a recipe file; a `str` is `TypeError` (pass `Path(...)`). Looks apply in call order. `px` is the unit of the recipe's lengths: `"design"`, image's design pixel (1/480 of the canvas's long side), or `"canvas"`, the scene's own canvas pixel (inside a group, the inner scene's); another word is `InvalidError` naming `px`, a non-`str` a `TypeError`. Rust: `Layer::look_in(Px::Canvas, json)`, `look_file_in`. See "Data in".

- **Space.** A look works on the layer's own fitted raster, not on the canvas: a vignette follows the layer's box, a
  centred warp its centre. A look on a group (`Layer.scene`) sees the whole group: its finished, isolated picture at the
  size the group is drawn at. A look on a fill sees a flat raster of the layer's size (a gradient map of a flat colour is flat;
  a look that depends on place, a scanline or a weave, is not). This keeps a layer's result independent of
  the others, so it caches by itself.
- **Design pixels** (C5b-lite). A look's `px` is image's design pixel: 1/480 of the canvas's long side, counted as the layer is
  fitted, before `place`. It is not a canvas pixel: on a 2,700 pixel canvas 1 px is 5.6 canvas pixels, so a ring about 25 canvas
  pixels wide is `width=4.5`. 2 px grain on a small inset is as coarse as on the photo behind it, whatever the inset's size.
  `render(size)` multiplies it, as image's `render(size=)` does. A layer that `place` then scales takes its grain and its
  halo with it. A scene's own lengths (`dx`, `fit` boxes, `feather`) are canvas pixels, and a look can count in them too:
  `layer.look(recipe, px="canvas")` makes 25 px 25 canvas pixels at any size, and inside a group the inner canvas's.
- **Light that spills** (C5c). A look sees the layer's colour over black. On a light layer the raster also grows by the look's
  **room**: how far image says the look paints past its picture (`Recipe::spread`, summed over the layer's looks, in the pixels
  of the canvas the look runs at), black and transparent round it. The box stays where it was, the halo shows past it, up to the
  room, and stage 7 makes it light. A layer that is not light keeps its source's alpha and has no room, so the halo a bloom adds
  outside the shape is cut, as it was. A glow is a reference made light:
  `Layer.of("logo").look(bloom).as_light().blend("screen")`. Darkness outside a shape (a drop shadow, an emboss shadow) is
  a layer of its own: a fill masked by the shape, `composed_world.py`, `chrome_type.py`. **No room, no error,** for a look image
  cannot bound (`Unbounded`: a warp about the centre, a statistic of the whole picture: `swirl`, `radial_blur`, `polar`,
  `auto_levels`, `equalize`, `dehaze`, `edges`...) or a layer that would pass image's 2^26 pixels with it; a layer whose looks
  spread none is untouched. **A look mixing a halo with an effect that uses the picture's bounds** (vignette, `x`/`y` centres, `lens_flare`, `gradient`, `chromatic_aberration`, noise and grain numbered by place, `tone_map`) places that effect by the layer's box, not the grown raster (image's `Options::with_room`). **Limit:** such a step after a halo step does not reach the halo (a vignette after a bloom leaves the halo undarkened): order the halo steps last.
- **Size and frame.** Scene owns the layer's size and fit: a recipe that sets `size` or `frame` is `InvalidError`. Regions
  are fine.
- **Maps.** A look's regions see the picture's `maps`. A look that needs a map (`subject`, `depth`, a `given` region) that
  the layer lacks, by name or as a hole, is `InvalidError` at render naming the layer, the step and the map. Scene runs no
  model and passes image no finder: a subject or depth map is found by the caller with image and handed in, so the error
  and every look are testable without a model.

## Recipes

Worked answers to what real pieces asked. Each is built from the calls above; none is a call of its own.

**Turn a label.** `place` turns a layer about its `anchor`, the centre of its box by default, so a text turned `-90` swings
about its middle and may leave the canvas. For a text the box's bottom left is the start of its baseline: pivot there, and
`dx`, `dy` put that point on the canvas.

```python
bts.Layer.text("LABEL", font="bricolage_grotesque", size=60).place(rotate=-90, anchor=(0, 1), dx=60, dy=380)
```

The label stands along the left edge on a 600 by 400 canvas: it reads upward from (60, 380), its letters' tops pointing
left, about 22 to 62 across and 218 to 379 down.

**A gradient on type.** `Layer.text` takes a flat colour only: its colour is a parameter of the scene (`<layer>.color`), and a
paint is vector's. Set the type with vector and give the drawing to a layer; the type keeps `place`, looks, masks and
`with_values` (its Params are `<layer>.drawing.<param>`).

```python
paint = btvec.linear_gradient([(0, "#ff2bd6"), (1, "#fff200")], start=(0, 100), end=(0, 280))
type_ = btvec.text("TYPE", font=btvec.Font.bundled("bricolage_grotesque"), size=200, anchor=(40, 260)).fill(paint)
bts.Layer.drawing(btvec.Drawing(type_, size=(W, H)))
```

**Trim a picture to its matte.** A `fit` or a flip is about the layer's box, which is the picture's, so a reflection under a
subject's feet needs the picture to end at the feet. Crop the photo and its matte to the matte's bounding box in numpy
before `Layer.picture` (rows and columns where the matte is above 0), then `fit` the cropped picture. Scene does not crop.

**A keyline round a subject.** A look paints on its own picture and cannot return its stroke alone, so draw the stroke on
a second, flat picture of the photo's size that carries the same matte as a map, and join it by `blend("lighten")` (or
`"screen"`): only the stroke differs from the flat colour. The look is image's `outline(field="subject", position="outside")`,
its `width` in image's design pixels (1/480 of the canvas's long side, "Looks"). The Conan poster of the demos builds exactly
this: a flat picture with the matte, the outline look, `lighten`, under the dog.

**A halo only.** A light reference's glow lies over its source, so it brightens the source too. Mask the glow by its inverted
source: the source stays as it was and only the halo is added.

```python
bts.Layer.of("sun").look(bloom).as_light().blend("screen").mask(bts.Layer.of("sun"), invert=True)
```

On a 400 by 300 canvas (background `#203050`, a disc `#ffe27a` of radius 44 drawn below the glow), the disc's centre pixel is
`#ffe27a` exactly (1.0, 0.886, 0.478) with the mask and (1.0, 1.0, 0.897) without it. 6 pixels past the edge it is
(0.67, 0.62, 0.46) in both, against the background's (0.13, 0.19, 0.31). The sun needs an alpha (a drawing, or a picture
with a matte): an opaque square picture masks the halo that falls inside its box (the pixel there stays the background).

**One look on the whole scene.** A look runs on a layer, and a scene has none of its own. Put the scene in a group and give
the group the look: it sees the finished picture.

```python
bts.Scene(W, H).add(bts.Layer.scene(inner).look(grade))
```

It equals the look applied to the flat render of `inner` (a `Layer.picture` of its pixels with the same look): on a 400 by
300 canvas of a fill, a drawing and a text, with a `gradient_map`, the largest difference is 0.0.

**`aspect` squashes toward the anchor.** `aspect` scales y about `anchor`, the centre of the box by default, so a contact
shadow `Layer.of("dog").place(aspect=0.12)` lands mid-body, not at the feet. Anchor it at the bottom edge and it keeps the
feet line, with no `dy`.

```python
shadow = bts.Layer.fill("#000000").mask(bts.Layer.of("dog").place(aspect=0.12, anchor=(0.5, 1)), feather=12)
```

A dog 200 pixels tall on rows 100 to 299 gives a shadow on rows 188 to 211 with the default anchor, rows 276 to 299 with
`anchor=(0.5, 1)`, and the same 276 to 299 with the default anchor and `dy=88` (half the height times 1 - `aspect`).

**Stroke widths under `perspective`.** A drawing is drawn flat and then warped, so a stroke grows with the magnification
where it lands: near lines are wider. For equal lines on the canvas, give the far lines a wide stroke and the near ones a thin
one (width by row, in the drawing); a faint wide halo (a second, wider stroke at low opacity) softens the taper of a line
along its run.

```python
bts.Layer.drawing(lines).perspective([(200, 50), (400, 50), (600, 400), (0, 400)])
```

Two horizontal lines of width 4 at rows 10 and 390 of a 400 by 400 drawing, warped to that quadrilateral on a 600 by 400
canvas, are 0.90 pixel thick at the far line (row 52) and 7.12 at the near one (row 372): a ratio of 7.9. Widths 15.76 and 2
give 3.56 and 3.58.

## Data in

| Layer | Takes | Scene needs of it |
|---|---|---|
| `look(recipe, px="design")` | an image `Recipe`; its `to_dict()`; a recipe file `Path` (read when called) | steps that are plain data, and no `size` or `frame`. A `Picture` or `Lut` parameter (`to_dict()` leaves the object) is `InvalidError` naming the step; give a path in a recipe file |
| `Layer.drawing(source)` | a vector `Drawing`; its `to_json()` text or dict; a drawing file `Path` (read when called) | a frame (`size`). A bare `Path` of the vector kind has no `to_json`: `TypeError`, wrap it in `btvec.Drawing(path)` |
| `Layer.picture(source)` | an array or a `Path` | in JSON a path with a digest |

A `pathlib.Path` is a file; a `str` is document text (a drawing's JSON) or a name (a bundled font). A look or drawing is checked
when it is given, by the crate scene links, so a typo fails at the call naming the layer and the step. A file given is read
once, there, and embedded: a saved scene is self-contained. A picture, a font file or a map file is not embedded; it is read
at `render`.

**Version skew.** A saved vector document carries `"version": 1`; an image recipe carries a `format`
(`Recipe.to_dict()` writes it, and scene keeps it). A reader refuses what the linked crate
does not read, naming where it is, the version found, the versions read and the sibling crate's version:

```
FormatError: /layers/2/drawing: vector document version <found>; this scene reads vector versions <read> (bullterrier-vector <version>)
FormatError: /layers/0/looks/0: image recipe format <found>; this scene reads image formats <read> (bullterrier-image <version>)
```

A scene itself carries `"version": 1`; its format changes only by a new number, and a release that does so keeps reading the
old ones.

## Values

`scene.parameters()` returns `dict[str, Parameter]` (an ordered dict), layer by layer: the layer's fields, its source's, its looks', its masks'.
The key is the address. `Parameter` is a frozen dataclass of the parameter format's fields with `address` standing for
`name` (a dotted address is not a valid `name`): `address`, `kind`, `meaning`, `default`, `unit`, `min`, `max`, `cyclic`,
`choices`, `nullable`, `min_length` and `value`, what the scene holds now (`Parameter.to_dict()` writes the format's form, `address` for `name`). Each range holds only valid values of that number alone, so a slider built
from it cannot ask for an error by itself. A rule across two numbers of an embedded look or drawing (image's `Band.low` at
most `high`) is the sibling's: `with_values` refuses it with the sibling's message.

| Address | Kind | Unit, range | Default (the field's own; `value` is what the scene holds) |
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
| `<layer>.mask.<field>` | | | a field of the mask layer (`mask.dx`, `mask.mask.dx`, `b.scene.t.mask.dx`); `mask#2` for the second call |

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
- Any other address is refused, naming it and listing the valid ones and the nearest; so is a value of the wrong kind (a bool for a number, a text for a bool). The addresses a layer lists are only those it has: a fill has no `text`, a reference no `picture`.

## Output

`render(size)`: `size` is the long side in output pixels (image's convention), a whole number from 1 to 65,000 with at most
2^27 pixels in all (a float32 RGBA array that size is 2 GiB; a larger picture goes straight to a file by bands, below); another is `InvalidError`
naming `size`. The canvas keeps its aspect: a canvas of 720 by 1280 at `size=1280` is 720 by 1280 pixels; each design pixel
is `size / max(width, height)` output pixels along the long axis; along the short axis the side is rounded up to whole
pixels and the picture stretched onto them, so a design pixel there is `rows / height` output pixels, which differs from
the long axis's by less than a pixel over the canvas, and visibly on a thin one ("What is built", Boxes).

The result is a new float32 array (height, width, 4), 0..1, rows top to bottom, straight alpha, transparent where nothing is
drawn. Colours are sRGB values. A smaller size shows the same picture in less detail. `render` reads the files the scene names
(pictures, fonts, maps) and writes none. A hole is `InvalidError` naming `<layer>.picture`; a file that is missing, unreadable
or whose digest differs is `ReadError`; an error from an embedded look or drawing at draw time is re-raised as `InvalidError`
naming the layer, with the sibling's message.

**Cache (C7).** Scenes derived from one another by `add` and `with_values` share a bounded in-process cache of what each layer drew ("What is built", Cache). The result depends only on the scene, its values, the size and the content of the files; caching skips work and does not change a result, with one limit: a file is keyed in the cache by its path, size and modification time, so a change that keeps all three is not seen. A saved scene's digest (computed at `to_json`, so a path set by `with_values` is digested then) is checked when a file is read at `render`, which does see such a change; a cached file is not read again. Same scene, values and size give the same array on the same machine and library versions, with or without the cache. A scene made apart (`Scene(...)` again, `from_json` again) shares nothing. The cache is observable through a plugin layer that counts its `draw` calls (`crates/core/tests/it/cache.rs`; the Python plugin API, [cache_probe.py](examples/cache_probe.py), does the same).

**PNG.** Rust: `Raster::write(path)` writes the picture as an 8-bit RGBA PNG (colour type 6, straight alpha, sRGB values) through image's writer, which rounds each level as `level * 255 + 0.5` cut to a whole number and embeds image's sRGB profile (the one deviation from "no profile": image's writer has no other mode). The extension picks the format as image's writer does (`.webp` lossless; a `.jpg` of a picture with alpha is refused); a failure is `Error::Write`. Python: `imageio.save(scene.render(1080), "poster.png")`; scene has no `render_png`, because the facade writes no file and `imageio` is image's one writer for Python.

**Banded output (B1).** `scene.render_bands(size, band_rows, sink)` (Python: `sink(first_row, band)`; Rust: `sink(rows, &Raster)`) draws the picture a band of `band_rows` rows at a time, top to bottom, the last band with what is left, and gives each to `sink`; the bands put together are `render(size)` bit for bit, whatever `band_rows`. The 2^27 pixels limit is on a band, so a picture past it can be drawn this way; the long side is at most 65,000. `band_rows` of 0, more than the picture's rows or a band of more than 2^27 pixels is `InvalidError` naming `band_rows`. A refusal of `sink` ends the render and is returned (Python: its exception is raised as it is). What is held is a band, the cache, and what the layers need whole: a plugin kind that does not say it draws by position (`LayerKind::by_position`; Python `by_position=True`) and a looked layer whose looks image cannot band (a whole-picture effect, a blur or `grain` that image sums by a Fourier transform, a layer not the canvas's size, a light layer) are drawn whole once and cut per band (their rasters stay resident, up to the cache's 2 GiB: the memory floor), a picture with no look is read where the band falls, a flat colour is one pixel, a picture, a group and a drawing are drawn for the rows the band reads and, with looks image can band, the margin the looks reach, and a mask is drawn for its band and the reach of its blur. `scene.picture_size(size)` is the picture's columns and rows without drawing. Rust writes the bands straight to a file: `scene.render_to_png(size, path)` (8-bit RGBA, the PNG `sRGB` chunk, the same levels as `Raster::write` rounded the same way; image's profile is not embedded) and `scene.render_to_pdf(size, path, &PdfOptions)` (the page and image of `write_pdf`, with no limit on the image's pixels; the compressed image goes to scratch files beside `path` while the file is made, so a band and a few buffers are held, never the picture or the compressed image). Each writes under a temporary name beside `path` and renames it when whole: a refusal leaves no file. A band is `BAND_PIXELS` (2^24, 256 MiB of floats) pixels: its rows are `2^24 / columns`. `bt-scene render` writes a picture of more than `BAND_PIXELS` pixels this way and a smaller one as before. An A0 print at 300 dpi (9,933 by 14,043 pixels) is written to a PNG in 25 s at a peak of 1.77 GB; `render` refuses it.

**DPI in a PNG or JPEG (print batch, 2026-11-17).** `raster.write_with(path, &WriteOptions::new().dpi(300.0)?)` and `scene.render_to_png_with(size, path, &options)` write the resolution as the PNG `pHYs` chunk (pixels per metre, `round(dpi / 0.0254)`: 300 dpi is 11,811, which a reader shows as 299.9994) or the JPEG's JFIF density; `dpi` is 1 to 65,535 (`InvalidError` naming `dpi`), a `.webp` holds none and is `Error::Write`. A `.jpg` of a fully opaque picture is written as colour (a picture with any alpha below 1 is refused, as before). `bt-scene render out.png|out.jpg --dpi N` does the same; the PDF keeps its own `dpi`. Python writes no picture file (`imageio.save(array, path, dpi=)` is image's).

**Mirrored bleed and registration marks (print batch).** `PdfOptions::bleed_fill(BleedFill::Mirror)` (`--bleed-fill mirror`) fills the bleed by folding the picture at its edge (the pixel past the edge is the edge pixel, the next the one inside it; a picture narrower than its bleed folds back and forth) instead of repeating the edge (`BleedFill::Repeat`, the default). `registration_marks(true)` (`--registration-marks`) draws four targets (a circle of 3 mm across crossed by two lines of 5 mm, 0.25 pt) centred in the 8 mm margin on the middle of each side of the trim, as crop marks do (both may be on). The PDF writer holds the first and last `bleed` rows while it streams. Bleed from content beyond the canvas stays open (`future.md`, O1).

**PDF.** Rust: `raster.write_pdf(path, &PdfOptions::new(dpi)?.bleed_mm(3.0)?.crop_marks(true))` writes a one-page PDF at a physical size; the command line is `bt-scene render scene.json out.pdf [--dpi N] [--bleed MM] [--crop-marks] [--flatten] [--cmyk-profile FILE]`. Python has no `render_pdf` (the same reason as `render_png`: the facade writes no file); the console script and `python -m bullterrier_scene render` are the Python user's route.

| Option | Meaning |
|---|---|
| `dpi` (required; 1 to 2400; the command line's default is 300) | Pixels per inch: the trim size is `width / dpi` by `height / dpi` inches, so 3600 pixels at 300 dpi are 12 in = 864 pt. `--size` stays the long side in output pixels, so a canvas of A3's proportions at `--size 4961 --dpi 300` is 3508 by 4961 pixels, 297 by 420 mm (its short side is rounded up to whole pixels, as in `render`). The scene canvas is the trim size; there is no paper size option |
| `bleed_mm` (0 to 50; default 0) | The page grows by this on every side with the rendered edge pixels repeated outward, or folded (`bleed_fill`) (the picture is the trim; a scene has nothing beyond its canvas to show). The repeat is whole pixels, rounded up (3 mm at 300 dpi is 36 pixels, 3.05 mm); `BleedBox` is exactly `bleed_mm` and the image is clipped to it. Suits a background that runs off the page; a detail at the edge is stretched, so draw the scene larger than the trim if the edge matters |
| `bleed_fill` (`Repeat` default, or `Mirror`; `--bleed-fill`) | What fills the bleed: the edge pixels repeated, or the picture folded at its edge |
| `registration_marks` (default off) | Four targets centred in the 8 mm margin on the middle of each side ("Mirrored bleed and registration marks"); the page grows as for crop marks |
| `crop_marks` (default off) | Eight black lines (0.25 pt wide, 5 mm long, vector paths), a horizontal and a vertical at each corner of the trim, starting 3 mm outside the `BleedBox`; the page grows by 8 mm on every side to hold them (in `DeviceGray`, so a CMYK page carries them too) |
| `flatten` (default off) | Composite over white (floats, straight alpha, then the PNG rounding) and write no soft mask. Otherwise the alpha is a `DeviceGray` soft mask, written only when some pixel is not opaque |
| `cmyk_profile(path)` | See below |

The page: `MediaBox` is the page (the `BleedBox` plus the crop-mark margin), `BleedBox` the trim grown by the bleed, `TrimBox` the picture at its physical size (`CropBox` is left unset); the boxes nest. The picture is one image XObject, 8 bits a channel (a level rounded as in the PNG: `level * 255 + 0.5` cut to a whole number), Flate (zlib level 6, no predictor), sRGB values in `DeviceRGB` (no profile is embedded), straight. No Info dictionary, date or ID: the same picture and options give the same bytes. A refusal is `InvalidError` naming `dpi` (not a number from 1 to 2400, or a page over 14,400 pt = 200 in, the limit of PDF readers: raise the dpi), `bleed_mm` (not from 0 to 50, or the image with its bleed over 2^27 pixels), `raster` (the picture alone over 2^27) or `cmyk_profile` (not an ICC profile, or not CMYK); `Error::Read` for a profile file that cannot be read; `Error::Write` for a path that cannot be written, and none is made. A page under 3 pt is written (a 1 by 1 picture at 300 dpi is 0.24 pt) though some readers refuse it.

**Limits of the PDF.** While a PDF is made, two scratch files (`.<name>.colour.part` and `.<name>.alpha.part`, as large as the compressed colour and alpha) lie beside it: the cost is disk, not memory; both are removed when the PDF is in place or the write fails. It is raster: vectors and text are rasterised at the render's size and are not kept as vectors (a PDF that keeps vectors is not planned: `future.md`). The bleed is the edge stretched, not more content. Colour is sRGB unless a CMYK profile is given.

**CMYK.** `PdfOptions::cmyk_profile(path)` (`--cmyk-profile FILE.icc`) reads a CMYK output profile the caller holds (none is bundled), converts the 8-bit sRGB values to it with `moxcms` (sRGB to the profile's PCS-to-device table, perceptual intent, 8-bit), writes the image as `[/ICCBased profile]` with `/N 4` and `DeviceCMYK` as the alternate, and embeds the profile bytes unchanged. The soft mask works as in RGB; crop marks stay `DeviceGray`. Limits: the intent is fixed at perceptual and there is no black point compensation or ink limit; a profile `moxcms` cannot convert to (it needs the profile's PCS-to-device table) is `InvalidError` naming `cmyk_profile`; the colours are converted before the bleed repeats them, so the bleed is the converted edge. Checked against macOS's Generic CMYK profile: white is about (1, 1, 0, 0) of 255, black a rich (188, 180, 164, 222), sRGB red (1, 214, 240, 3) ("Print output", design-decisions).

## Plates

`Layer::inks(["pink", "blue"])` (Python `layer.inks("pink", "blue")`) says which inks a layer prints in: a layer whose last look separates inks (`halftone` or `dither` with a `separation`: image's `ink_planes`) names one ink for each plane, in the order the look makes them; a layer with no look is one shape in one ink, where its alpha is. A name is letters, digits and `_`, starting with a letter; one to eight, each once (`InvalidError` naming `inks`). The picture does not change; the names are in the saved scene (`"inks": [...]`) and in `==`.

`scene.plates(size)` (Python `Scene.plates(size, underbase=None)`) gives `Plates`: `names()`, `width()`, `height()`, `plane(name)` (float coverage 0 to 1, `width * height`, rows from the top; Python a float32 array). Plate `i` is the render of the layers of ink `i` alone, each as the shape of its ink (the plane its look separates, or its alpha) placed, masked and faded as in the picture, joined normally, so the same ink overprints as `a + b (1 - a)`. A layer's blend mode is not asked; layers of different inks lie one over the other as inks do, so the plates make the picture where the scene's layers multiply: `paper * prod (1 - coverage * (1 - ink / paper))`, the rule image's spot separations print by (tested to 1e-4). Inks are in the order they first appear, bottom layer first. Planes are held as floats: columns times rows times inks is at most 2^27 (refused naming `size` before any drawing).

```text
let plates = scene.plates(3000)?;
plates.write_png(Path::new("out/poster"), &options)?;   // out/poster-pink.png, ... (grey, black = ink)
plates.write_pdf(Path::new("out/plates.pdf"), &options)?; // a page for each plate, labelled by its ink
// options = PdfOptions::new(300.0)?.bleed_mm(3.0)?.bleed_fill(BleedFill::Mirror).crop_marks(true).registration_marks(true)
```

A plate file is its plane with the options' bleed, then, with either mark on, 8 mm of margin with the marks, black, at the same pixels on every plate; the PNG says the options' `dpi`. A PDF page is the page `write_pdf` makes (the boxes, the marks as paths) with the plate as one 8-bit `DeviceGray` image. `cmyk_profile` is refused (`InvalidError` naming it) and `flatten` is ignored. Python: `bts.PdfOptions(dpi=300, bleed_mm=3, bleed_fill="mirror", crop_marks=True, registration_marks=True)`, `plates.write_png(stem, options)` and `plates.write_pdf(path, options)` (the one place Python writes a file: a plate's marks and pages are scene's drawing).

**White underbase.** `scene.plates_with_underbase(size, &Underbase::new("#15141b")?.choke(0.8)?.tolerance(0.04)?.feather(0.6)?)` (Python `underbase=bts.Underbase("#15141b", choke=0.8, ...)`) puts a plate named `white` first. Its coverage is: where the union of the inks is at least half, how far the pixel's centre is from the nearest bare pixel's (an exact Euclidean distance) less `choke`, held between 0 and 1 (so a pixel next to bare paper is whole at a choke of 0, none at 1; the canvas edge is not bare); times one minus the knockout, which is 1 where the scene's picture over the garment colour is within `tolerance` of it (the distance between two sRGB colours over the length of the cube's diagonal, 0 to 1), blurred by `feather` (a blur of that standard deviation, as a mask's). `choke` and `feather` are design pixels. The name may not be an ink's (`InvalidError` naming `name`).

Rust has `scene.plates(size)` and `scene.plates_with_underbase(size, &underbase)`; Python has one call, `scene.plates(size, underbase=None)`, the scene's `_with` convention folded into a keyword. A garment colour is `#rrggbb` (no alpha: `InvalidError` naming `garment`). Python's `PdfOptions` and `Underbase` check their values where they are made.

Limits: a reference (`Layer.of`) does not inherit the inks of the layer it names (inks are part of the layer's own join, stages 6 to 9): it prints in none unless it names its own; inks inside a group are not handed on (the group is one shape); a light layer cannot name inks; a look that separates nothing is refused in image's words (`inks`); halftone's bare-paper specks are not on a plate (the picture has them: `specks` 0 for plates equal to the picture); an underbase is on or off at coverage 0.5, not a halftone; each plate draws its layers again (a looked layer runs its recipe once for each of its inks, through the cache for what is below it).

## Saved scene

`to_json()` writes one self-contained document; `Scene.from_json(text, base=folder)` reads it back and the two give the same
pictures: `from_json(to_json(s)) == s`, the same bits at every size, the same `parameters()`. The text is compact and the
same on every call (keys in the order below; an object that is data, a text option or a look, keeps the order it was given).
`"version": 1`; a release that writes a new version keeps reading this one.

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

**A layer** holds, in this order, a key only where it differs from its default (a key with its default value is accepted on
reading): `name`; one source: `picture` (a file, or `null`, a hole) with its `maps` (name to file or `null`), `drawing`
(vector's document inline, with `values`: the Params that were set, name to number), `text` with `font` (a bundled font's
name, or a file) and `color` and `options` (vector's text options as given), `fill` (a colour), `scene` (the inner document
inline: its own `version`, `canvas` and `layers`), `make` (a plugin kind: `{"kind": name, "options": {...}}`, read through the registry) or `of` (a reference: the name; it takes `place`, `masks`, `looks` and the join
fields, and no other); then `fit` (`{"box": [x, y, width, height], "how": "cover"|"contain"|"stretch", "align": [x, y]}`,
`how` and `align` only when they are not `cover` and the centre), `place` (the fields `dx`, `dy`, `scale`, `aspect`, `rotate`,
`skew_x`, `skew_y`, `flip_x`, `flip_y`, `anchor`: those that differ) or `perspective` (four `[x, y]` corners), `opacity`,
`blend` (a CSS name), `light`, `masks` and `looks`. **A mask** is `{"layer": {...}, "use": "luminance", "feather": 8, "invert": true}` (the mask layer
is a layer document with no name) or `{"map": "subject", ...}`, in the order of the `mask` calls, whichever kind (`mask`,
`mask#2`... are numbered by that order). **A look** is `{"format": 3, "recipe": {...}}`: image's own document, `format` the
lowest image format the recipe needs; a look in canvas pixels adds `"px": "canvas"`.

**Files.** A picture, a map or a font file is `{"path": ..., "sha256": ...}`. `to_json` writes the path as the scene holds it
(relative to the working folder if it was given so, absolute if it was) and the digest of the file as it is now (so a path set
by `with_values` is digested at that call; `ReadError` if the file cannot be read; `InvalidError` naming
`<layer>.picture` for a picture or a map held in memory: it has no file). `from_json` resolves a relative path against `base`
(the folder of the scene file; default the working folder) and keeps the digest: a picture and a map are read, and the digest
checked, when the scene renders (`ReadError` naming both digests; a missing file is `ReadError` at `render`, never at load). A
scene read with `base` keeps its paths relative, and `parameters()` lists a picture's value as the path it is read from (`base`
joined). A font file is read at load (the box of a text needs the font), digest checked. The digest may be left out of a hand-written document.
**A path where a look or a drawing belongs** (`"looks": ["looks/night.toml"]`, `"drawing": "logo.json"`) is read when the scene
is loaded and written inline by the next `to_json()`.

**Errors.** `FormatError` (also a `ValueError`) starts with the JSON Pointer of the value that is wrong and says what is
wrong: text that is not JSON (the pointer is empty), an object that writes a key twice (the pointer of the second), a `version`
this release does not read (found and read), a key it does not know (the keys and the nearest), a value of the wrong kind or
range, a layer the scene refuses (a reference to nothing or that leads back to itself, at its `of`, `/layers/1/of`; a name used twice,
more than `MAX_LAYERS` layers, deeper than `MAX_DEPTH`, at `/layers`; a scene or a mask layer nested deeper than `MAX_DEPTH` in the document is refused at the pointer of the one that is too
deep, before the reader recurses further), a kind the registry does not hold (`/layers/1/make/kind`, listing the kinds and the nearest) or options it refuses (`/layers/1/make/options`), a look of a `format` image does not read (`/layers/0/looks/0/format`, naming the formats read) and a
drawing of a vector version vector does not read (`/layers/0/drawing/version`, in vector's words). A hostile document is bounded: the reader
is the parser of `serde_json` (it stops a nest at 128 levels), a depth bound of its own, and the count of layers is checked before any is
built; a document is read in a thread of 256 KB.

## Errors

| Error | When | Carries |
|---|---|---|
| `BullterrierSceneError` | the base of every error scene raises for a caller's input | |
| `InvalidError` | a value or argument scene refuses, also a `ValueError`; at the call that got it, else at `render` | the argument or address, the rule and the value; for an embedded look or drawing, the layer and the sibling's message |
| `FormatError` | a document `from_json` cannot read; also a `ValueError` | the JSON Pointer; a version skew names versions as above |
| `ReadError` | a file the scene names cannot be read, or its digest differs; also an `OSError` | the layer, the path, and for a digest both digests |

| `WriteError` | a file cannot be written (`Plates.write_png`, `Plates.write_pdf`; Rust `Error::Write`); also an `OSError` | the path and the reason |

A wrong type in the calling code (`Layer.drawing(path)`, `Layer.picture("a.jpg")`, `add(scene)`) is a plain `TypeError`
naming the fix. Plain numbers are checked when a call is made; a hole, a map's size and files when the scene renders. Depth
is bounded (`MAX_DEPTH`, `MAX_LAYERS`); the walks over references are iterative, and the walks over masks (drawing, `Clone`,
`Drop`) recurse no deeper than `MAX_DEPTH`, so none needs more than a 256 KB thread stack (tested at that size).

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
| Options | keyword arguments | a value named after the call, built with `new` and setters: `Place::new().dx(100.0).rotate(Deg(-4.0))`, `Mask::new(by).using(Use::Luminance).feather(8.0).invert()` (`use` is a keyword); the required argument alone converts into it: `layer.mask(by)`. Text: `Text::new("bricolage_grotesque").color("#ffffff").option("size", 84.0)` and `Layer::text(content, &text)?`, each option plain data (`Into<serde_json::Value>`) |
| Modes | strings: `"cover"`, `"screen"`, `"light"` | enums: `Fit::Cover`, `Blend::Screen`, `Use::Luminance` |
| Values | `scene.with_values({"title.text": "Pilot"})` | `scene.with_values(&Values::new().set("title.text", "Pilot"))?` |
| Output | a numpy array; `imageio.save` writes it | a `Raster` of the output size; `raster.write(path)` writes a PNG, `raster.write_pdf(path, &PdfOptions::new(300.0)?)` a PDF |
| Command line | `bt-scene` (the Rust command line, run in-process) | `bullterrier_scene_cli::main(args, version, out, err)` |
| Errors | exceptions | `Result<_, bullterrier_scene::Error>`. One rule: a call that can be refused returns `Result` (`add`, `named`, `fit`, `place`, `perspective`, `opacity`, `look`, `mask`, `with_values`, `render`, the constructors that parse); a call that cannot (`blend`, `as_light`, `Layer::of`, `Layer::scene`) returns the value. A scene never defers an error. `Error` is `#[non_exhaustive]`: `Invalid { address, message }`, `Format { pointer, message }`, `Read { path, message }` |
| Plugin kinds | an entry point of `bullterrier_scene.layers`; `bts.register_layer(kind, draw)` | `LayerKind`, with the one question "draw this rectangle at this scale": `Layer::make(Arc::new(kind))`; by name through a `LayerKinds` registry of `LayerKindFactory`s |

[cover_template.rs](../crates/core/tests/it/cover_template.rs) is the cover template in Rust (a test module: `saved.rs` runs it).

## What the demos asked

Every row of "What the demos and the experiment asked of scene" in [roadmap.md](roadmap.md), and where an example covers it.

| Need | Covered by |
|---|---|
| Move, scale and stand a cut-out subject on a floor | `fit(..., align=(0.5, 1.0))`: [composed_world.py](examples/composed_world.py), `dog` |
| An effect on one layer only: the logo's glow must not light the windows; the sun glows, the dog does not | a reference with a look, made light: [mill.py](examples/mill.py) `logo_glow`, [composed_world.py](examples/composed_world.py) `sun_glow`, [wow_poster.py](examples/wow_poster.py) `rings_glow` |
| Contact shadow and floor reflection of a cut-out | references to the dog, placed and masked: [composed_world.py](examples/composed_world.py) `reflection`, `shadow` |
| Reframing to the delivery shape without an outside crop | `fit(box, "cover", align=...)`: [mill.py](examples/mill.py) `night` |
| One colour typed once, for vector, image and the caller's code | in part: a scene's own colour is one parameter ([cover_template.py](examples/cover_template.py) `band.color`); inside a look or a drawing it waits for a colour reference in image and vector |
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
- **No global state.** No current scene, canvas, font or random generator; the one registry of Python layer kinds is process-wide (in Rust a `LayerKinds` is a value). The cache is keyed by content, not held by the caller.
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
| vector | a public list of the document versions it reads and a public crate version, for the version-skew message | C6 |
| image | a look refused for its picture's size says "`frame` makes a picture of ..." when no frame is set, and a missing map's message names image's Rust API (`Photo::with_map`); scene passes both through (the plan said `Options::with_map`: at the pin it is `Photo::with_map`, a method of the photo, which is what scene calls) | C3 |
| image | a public call that returns a region of a look as a map (today `mill.py` matches a `trace` stage by its label): not met, and C3 did not need it (maps are handed in) | M3 |
| C6 | **Met (C6a).** A group's address space (`badge.scene.label.text`) is a second name lookup; `with_values` rebuilds the inner scene (not mutated) and re-resolves it, and the outer scene again | C6a |
| image | **Met (image `7f91eef`, C5c).** `Recipe::spread` and `Spread::pixels`: how far a look paints past its picture. `Options::with_room` (a render on a grown picture whose bounds effects keep the original box): **met**, so a halo and a vignette share a layer | C5c |
| C7 | **Met (C7).** A group's cache key is its inner layers, the density per axis and the phase; a reference to a group at an unchanged density shares the raster | C7 |
| C6 | **Met (C6a).** The address walk of `<layer>.look#2...` reads the layer's one `Finish`; a reference's own looks are numbered from 1 | C6a |
| C6 | **Met (C6a)** by the number each mask stores (`mask#n` is the nth call); a map mask has no parameter | C6a |
| C7 | **Met (C7).** A mask layer shared by several layers, or drawn again by a reference, is drawn once for a target: the key of a mask plane is the mask layer, its reading, its feather, its inversion and the target | C7 |
| the ecosystem parameter format | parameter format v4: an `address` where `name` stands (its schema pattern `^[A-Za-z][A-Za-z0-9_]*$` refuses a dotted address); a `color` that may carry an alpha (`#rrggbbaa`, which a fill and a text take); a unit for a plain number (vector's `""`, written `count` here) | C6a |
| ~~image~~ | (done: `with_window`, `band_margin`, and the exact-only `band_margin`, image `4997d954`: it answers none for a Fourier-summed blur, `grain` included) position-aware effects for banded looks: an effect that takes the row offset of the band and the canvas size (image's `Place { canvas_height, canvas_width, first_row }` is internal, `Options` has no first-row setter), and position-hashed noise for the effects that draw one seeded stream (grain, dust, scratches): 40 of image's 62 effects are whole-picture, so a layer with a look is drawn whole in a banded render | B1 (a later batch) |

## Questions settled

1. **`MAX_DEPTH` (16)** is measured for masks in C3, references in C4b and scenes in C4c ("What is built", Depth and count). `MAX_LAYERS` and 2^27 pixels are measured ("What is built").
2. **The cache's bound** is 2 GiB, measured on a 15-layer A3 poster ("What is built", Cache; "The cache" in design-decisions).
