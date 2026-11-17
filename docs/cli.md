# The command line: `bt-scene`

A thin layer over the library: it reads a saved scene (`docs/api.md`, "Saved scene"), sets values, renders. The command line is
Rust (`crates/cli`); the Python console script `bt-scene` and `python -m bullterrier_scene` run it in-process.

```
bt-scene render SCENE OUT.png|OUT.pdf [--size PIXELS] [--set ADDRESS=VALUE]... [--base DIR]
                [--dpi N] [--bleed MM] [--bleed-fill repeat|mirror] [--crop-marks] [--registration-marks] [--flatten] [--cmyk-profile FILE.icc]
bt-scene parameters SCENE [--json] [--base DIR]
bt-scene validate SCENE [--base DIR]
bt-scene --version | --help
```

| Exit | Meaning |
|---|---|
| 0 | success |
| 1 | the command failed: one line `error: <the library's message>` on stderr, never a traceback |
| 2 | wrong usage: an unknown command or option, `--set` without `=`, a `--size` that is no number (the message explains) |
| 130 | Ctrl-C (the console script) |

Results go to stdout, errors to stderr.

- **`render`** writes `OUT` and prints its path: by the extension (`.pdf` in any case, else the picture writer's), `Raster::write`
  (an 8-bit RGBA PNG) or `Raster::write_pdf` (a raster PDF; "Output" in `api.md`). `--size` is the long side
  in pixels, default the canvas's long side; 1 to 65,000 (the library's limit: another is exit 1 naming `size`).
  **A picture of more than 2^24 pixels (`BAND_PIXELS`, 4,096 squared) is drawn and written by bands** (`Scene::render_to_png`,
  `render_to_pdf`): it is never held whole, so a print size past the 2^27 pixels of `render` (A0 at 300 dpi, 9,933 by 14,043
  pixels) is written. A smaller one is written as before. The band height is not a flag: it is as many rows as 2^24 pixels hold
  ("Banded rendering" in `design-decisions.md`). The two files differ in how they say sRGB (a banded PNG carries the PNG `sRGB`
  chunk, a whole one image's profile) and in the compression, not in a pixel.
- **PDF flags** (any of them with an output that is not `.pdf` is exit 1 naming the flag): `--dpi N` pixels per inch, so the page is
  the picture's size over N inches (1 to 2400, default 300); `--bleed MM` 0 to 50 mm of bleed, the rendered edge repeated outward
  (default 0); `--bleed-fill mirror` folds the picture at its edge for the bleed (default `repeat`); `--crop-marks` and `--registration-marks` marks outside the bleed; `--dpi` also writes the resolution into a PNG or JPEG (1 to 65,535); `--flatten` the picture over white instead of a soft mask; `--cmyk-profile FILE.icc`
  CMYK through the caller's output profile, embedded (none is bundled). A canvas of A3's proportions at `--size 4961 --dpi 300` is a
  297 by 420 mm page.
- **`--set`** sets a parameter by address before rendering (`parameters` lists them). VALUE is read by the address's kind: a
  number (`0.5`), `true` or `false`, a colour (`#rrggbb`), a text or a choice's name, a picture's file path (relative to the
  working folder). An address the scene does not have is refused with the nearest. All values are set or none.
- **`--base`** is the folder the scene's relative paths are relative to; default the scene file's folder.
- **`parameters`** prints a table (address, kind, unit, range, current value); `--json` prints the parameter format's list of
  objects (`Parameter.to_dict()` of the Python package; a choice's names are there).
- **`validate`** reads the scene and renders it at one pixel without writing anything: the document must read, every file must be
  there with the digest the scene was saved with, every layer must draw (a hole and a map of another size fail). Exit 0 prints
  `ok: <canvas>, <layers>`.

## A session

`docs/examples/cover.scene.json` is a saved scene (320 by 180: a synthetic photo, a band, a title; made by
`python docs/examples/c6b_saved_scene.py docs/examples`), with its photo `cover-photo.png` beside it.

```console
$ bt-scene validate docs/examples/cover.scene.json
ok: 320 x 180, 3 layers

$ bt-scene parameters docs/examples/cover.scene.json | grep -E "^(ADDRESS|band\.(opacity|color)|title\.text)"
ADDRESS        KIND     UNIT      RANGE              VALUE
band.opacity   float    fraction  0..1               0.82
band.color     color                                 #101018
title.text     text               1+ characters      Saved scene

$ bt-scene render docs/examples/cover.scene.json out.png --size 640 \
    --set title.text="From the command line" --set band.opacity=0.6 --set band.color="#2b0a3d"
out.png

$ bt-scene render docs/examples/cover.scene.json out.png --set band.opacty=0.5; echo "exit $?"
error: band.opacty: no such address in this layer; its addresses are: opacity, blend, light, dx, dy, scale, aspect, rotate, skew_x, skew_y, flip_x, flip_y, color (did you mean opacity?)
exit 1
```

The second render is [gallery/p1-cli.png](gallery/p1-cli.png). In a checkout run the commands as `uv run bt-scene ...`.

A print: the same scene as a PDF with 3 mm of bleed and crop marks, and its first page as a picture (macOS: `qlmanage`; elsewhere
`pdftoppm -png -r 150`):

```console
$ bt-scene render docs/examples/cover.scene.json cover.pdf --size 640 --dpi 150 --bleed 3 --crop-marks
cover.pdf
$ qlmanage -t -s 1000 -o . cover.pdf        # writes cover.pdf.png
```

On the page the picture runs 3 mm past the trim on every side (the edge stretched), and the crop marks stand outside it, at the trim
lines. (It is not a gallery piece: `poe look` writes the gallery from Python, which has no PDF reader.)
