//! Where a layer lands: the output grid it is drawn for (the canvas, or the canvas and a margin for a mask whose blur reaches
//! past it), and the one warp that puts the layer's drawn raster on it. The renderer lays layers over one another through it,
//! and a mask draws its layer through it onto a plane of its own.

use crate::Error;
use crate::geometry::{MAX_LOOKED_PIXELS, MAX_PIXELS, Rect, Scale};
use crate::layer::Body;
use crate::placement::{Layout, Placement};
use crate::source::{Drawn, Request};
use crate::warp::{Map, is_identity, warp_into};

/// Where a layer is drawn: the output grid of the canvas, with `margin` more pixels on each side (along x and y) for a mask
/// whose blur reaches past the canvas.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target {
    /// The scene's canvas, in design pixels.
    pub(crate) canvas: Rect,
    pub(crate) scale: Scale,
    /// The canvas's size in output pixels.
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) margin: (usize, usize),
    /// How far, in output pixels, the canvas's top left lies from the grid's (margin not counted): 0 for the whole canvas, and
    /// the part of a pixel a scene used as a layer starts at when its box is not on a pixel.
    pub(crate) offset: (f64, f64),
    /// The pixels the scenes drawn round this one hold in their own rasters while it is drawn: 0 for the whole canvas.
    pub(crate) held: usize,
}

impl Target {
    /// The size in pixels, margin included: `(columns, rows)`.
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.columns + 2 * self.margin.0, self.rows + 2 * self.margin.1)
    }

    /// This target with `reach` more pixels of margin on each side, along x and y.
    pub(crate) fn grown(&self, reach: (usize, usize)) -> Self {
        Self {
            margin: (self.margin.0 + reach.0, self.margin.1 + reach.1),
            ..*self
        }
    }

    /// The part of the canvas plane the target holds, in design pixels: the canvas grown by the margin.
    fn rect(&self) -> Rect {
        // a margin of at most 2^27 pixels is exact in a float
        #[expect(clippy::cast_precision_loss, reason = "a margin of at most 2^27 pixels")]
        let (across, down) = (
            self.margin.0 as f64 / self.scale.x.pixels(1.0),
            self.margin.1 as f64 / self.scale.y.pixels(1.0),
        );
        Rect {
            x: self.canvas.x - across,
            y: self.canvas.y - down,
            width: self.canvas.width + 2.0 * across,
            height: self.canvas.height + 2.0 * down,
        }
    }
}

/// A layer drawn at the scale it will have and ready to be put where it belongs: the raster, and the one warp that puts it on
/// the target.
pub(crate) struct Landed {
    pub(crate) drawn: Drawn,
    pub(crate) map: Map,
    pub(crate) clip: Rect,
    pub(crate) size: (usize, usize),
}

impl Landed {
    /// Whether the raster is the target as it is (an unmoved layer the size of the output).
    pub(crate) fn is_unmoved(&self) -> bool {
        matches!(self.map, Map::Affine(affine) if is_identity(&self.drawn.raster, affine, self.clip, self.size.0, self.size.1))
    }

    /// Gives `put(index, pixel)` each pixel (straight RGBA) the layer lands on, `index` being its place in the target, row
    /// after row. A pixel the layer does not reach is not given.
    pub(crate) fn each(&self, put: &mut impl FnMut(usize, [f32; 4])) {
        if self.is_unmoved() {
            for (at, pixel) in self.drawn.raster.pixels().as_chunks::<4>().0.iter().enumerate() {
                put(at, *pixel);
            }
        } else {
            warp_into(&self.drawn.raster, self.map, self.clip, self.size, put);
        }
    }
}

/// `body` drawn at the scale it will have, and put on `target`: the source asked for the part of its box that shows, then the
/// map of the one warp worked out.
pub(crate) fn land(body: &Body, target: &Target, light: bool) -> Result<Landed, Error> {
    // a source with no box of its own (a fill) takes the part of the canvas the target holds, margin included, so that a mask
    // that fills the canvas goes on into the margin; but only where nothing acts on the box, for `fit` and `place` work on
    // it and a layer must be the same shape whatever its blur adds
    let unplaced = body.fit.is_none() && body.placement == Placement::default() && body.extras.is_empty();
    let opened = body.source.open(if unplaced { target.rect() } else { target.canvas })?;
    let layout = Layout::new(body.fit, body.placement, &body.extras, opened.own, target.scale).offset(target.offset);
    let request = Request {
        held: target.held,
        canvas: (target.canvas.width, target.canvas.height),
        ..layout.request(
            opened.own,
            opened.whole,
            if body.finish.is_looked() {
                MAX_LOOKED_PIXELS
            } else {
                MAX_PIXELS
            },
        )
    };
    // the one place the drawn-size limit is checked, before any source draws
    request.scale.size(request.window)?;
    let drawn = opened.draw(request, &body.finish, light)?;
    let (map, clip) = layout.warp(&drawn);
    // the output starts `margin` pixels, and the offset, up and left of the canvas
    #[expect(clippy::cast_precision_loss, reason = "a margin of at most 2^27 pixels")]
    let (across, down) = (
        target.margin.0 as f64 + target.offset.0,
        target.margin.1 as f64 + target.offset.1,
    );
    let map = if (across, down) == (0.0, 0.0) {
        map
    } else {
        map.shifted(across, down)
    };
    Ok(Landed {
        drawn,
        map,
        clip,
        size: target.size(),
    })
}
