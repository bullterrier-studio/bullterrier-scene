# Dither, halftone and print looks

Planned 2026-11-15, not scheduled. Scene's part of print output for dither, halftone and spot-ink pieces, the looks the
dither tools in common use print from. Built today (O1): a raster PDF with DPI, bleed by edge repeat, crop marks and a
caller's CMYK ICC profile. The ecosystem view is the design repository's `dither-and-print.md`.

| Item | What it is | Needs |
|---|---|---|
| Plates | One file per ink (a grey PNG, or a PDF page per plate named by its ink), with registration marks, at the PDF's DPI, beside the composite proof. | image: each ink's coverage plane from `halftone` and `dither` (planned there). A layer whose look separates inks hands its planes on. |
| Mirrored bleed | Bleed filled by mirroring the edge, as an option next to the edge repeat; bleed from content past the canvas stays open (`future.md`, O1). | none |
| DPI in PNG and JPEG | `Raster::write` passes a DPI to image's writer, which owns the `pHYs` chunk and the JPEG density. The PDF keeps its own. | image: the DPI write option (planned there) |
| White underbase | For printing on dark garments and film: a white plate under the inks, the union of their coverage choked by a width, knocked out where the garment's own colour should show (a colour, a tolerance, a feather). Built from the plates and the masks scene has. | Plates |

Not scene's: the screens, dithers and inks themselves (image), tracing them to SVG (vector), and anything that changes over
a sequence (video).
