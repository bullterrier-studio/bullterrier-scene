# Gallery

One or two pictures per batch, written by `uv run poe look` from `docs/examples/` (the same call writes the larger ones to
`dist/look/`). Every picture here is generated in code.

| Picture | Batch | What it shows |
|---|---|---|
| ![the first picture](c1-first-picture.png) | C1 | A synthetic dusk photo with one image look (cooler, darker, a vignette, fine grain) and a vector headline drawn over it, rendered whole at 480 pixels by `c1_first_picture.py` |
| ![a pile of prints](c2-pile-of-prints.png) | C2 | Six synthetic snapshots on a table, each `fit` into its own frame (a wide photo cut by `cover` into a tall one, a tall photo kept whole by `contain` with paper either side, a square squashed by `stretch`) and then turned, skewed, mirrored or scaled about its frame by `place`; each has a paper margin and a shadow that is the same placement moved. `c2_prints.py`, 800 pixels |
| ![one tall photo cut three ways](c2-delivery-crop.png) | C2 | The delivery crop: one 900 by 2000 photo `fit` with `cover` into a 720 by 1280 frame three times, `align` 0, 0.5 and 1 on the long side, so the top, the middle or the bottom stays and the rest is cut at the frame's edge. `c2_prints.py`, 760 pixels |
