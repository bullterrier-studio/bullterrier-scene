# Dither, halftone and print looks

Built (O1, then the print batch **done 2026-11-17**): a raster PDF with DPI, bleed by edge repeat or mirror, crop and registration marks and a
caller's CMYK ICC profile; plates; DPI in PNG and JPEG; a white underbase. The calls are "Plates" and "Output" in [api.md](api.md), the reasons "Print: plates..." in [design-decisions.md](design-decisions.md).

| Item | What it is | Needs |
|---|---|---|
| ~~Plates~~ **Done 2026-11-17** | One file per ink (a grey PNG, or a PDF page per plate named by its ink), with registration marks, at the PDF's DPI, beside the composite proof. | image: each ink's coverage plane from `halftone` and `dither` (planned there). A layer whose look separates inks hands its planes on. |
| ~~Mirrored bleed~~ **Done 2026-11-17** | Bleed filled by mirroring the edge, as an option next to the edge repeat. | none |
| ~~DPI in PNG and JPEG~~ **Done 2026-11-17** | `Raster::write` passes a DPI to image's writer, which owns the `pHYs` chunk and the JPEG density. The PDF keeps its own. | image: the DPI write option (planned there) |
| ~~White underbase~~ **Done 2026-11-17** | For printing on dark garments and film: a white plate under the inks, the union of their coverage choked by a width, knocked out where the garment's own colour should show (a colour, a tolerance, a feather). Built from the plates and the masks scene has. | Plates |

Not scene's: the screens, dithers and inks themselves (image), tracing them to SVG (vector), and anything that changes over
a sequence (video).
