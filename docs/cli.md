# The command line: `bt-scene`

A thin layer over the library: it reads a saved scene (`docs/api.md`, "Saved scene"), sets values, renders. The command line is
Rust (`crates/cli`); the Python console script `bt-scene` and `python -m bullterrier_scene` run it in-process.

```
bt-scene render SCENE OUT.png [--size PIXELS] [--set ADDRESS=VALUE]... [--base DIR]
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

- **`render`** writes `OUT` (`Raster::write`: an 8-bit RGBA PNG; "Output" in `api.md`) and prints its path. `--size` is the long side
  in pixels, default the canvas's long side; 1 to 65,000 (the library's limit: another is exit 1 naming `size`).
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
