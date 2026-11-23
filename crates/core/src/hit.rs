//! Which layer shows at a pixel of a picture: an editor's hit test. Each layer is drawn alone, top first, for the one row of
//! the picture the pixel is in, by the renderer's own `lay`, so placement, perspective, looks, masks, opacity and light are
//! those of the render.

use std::sync::Arc;

use crate::cache::Cache;
use crate::landing::Target;
use crate::render::{lay, picture_size, target_of};
use crate::source::scene::Group;
use crate::{Error, Layer};

/// The least alpha a layer leaves at a pixel to be at it: the alpha an 8-bit picture keeps as 1 of 255 (`level * 255 + 0.5`
/// cut, as `Raster::write` rounds), so a layer is at the pixels where it shows at all.
const SEEN: f32 = 0.5 / 255.0;

/// The address of the topmost of `layers`, on a canvas of `width` by `height`, at the pixel (`x`, `y`) of the picture at
/// `size`, or `None`.
///
/// # Errors
/// [`Error::Invalid`] naming `size` for a size outside 1 to 65,000, `x` or `y` for a pixel outside the picture; whatever a
/// layer cannot draw, naming the layer.
pub(crate) fn layer_at(
    (width, height): (u32, u32),
    layers: &[Layer],
    (x, y): (i128, i128),
    size: i128,
    cache: &Cache,
) -> Result<Option<String>, Error> {
    let (columns, rows) = picture_size(width, height, size)?;
    let column = within("x", x, columns)?;
    let row = within("y", y, rows)?;
    let target = target_of(width, height, (columns, rows), (row, 1), cache);
    cache.begin_render(false);
    topmost(layers, &target, column, &Ok)
}

/// `at` as a pixel index from 0 to `count - 1`, or an error naming `field`.
fn within(field: &str, at: i128, count: usize) -> Result<usize, Error> {
    usize::try_from(at).ok().filter(|&at| at < count).ok_or_else(|| {
        Error::invalid(
            field,
            format!("{field} is a pixel of the picture, from 0 to {}, got {at}", count - 1),
        )
    })
}

/// The address of the topmost of `layers` that leaves alpha at `column` of the row `target` draws, each drawn as `alone` makes it
/// (the layer itself on the canvas; inside a group, the group with that layer alone in it, and so on out).
fn topmost(
    layers: &[Layer],
    target: &Target,
    column: usize,
    alone: &dyn Fn(Layer) -> Result<Layer, Error>,
) -> Result<Option<String>, Error> {
    for (index, layer) in layers.iter().enumerate().rev() {
        let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
        let mut row = vec![0.0_f32; target.drawn_size().0 * 4];
        lay(&alone(layer.clone())?, target, &mut row).map_err(|error| error.in_layer(&name))?;
        if row[column * 4 + 3] < SEEN {
            continue;
        }
        if let Some(group) = layer.drawable()?.source.group() {
            let inner = |inner: Layer| alone(layer.with_source(Arc::new(Group::new(group.drawing(vec![inner]))))?);
            if let Some(found) = topmost(group.drawn(), target, column, &inner)? {
                return Ok(Some(format!("{name}.scene.{found}")));
            }
        }
        return Ok(Some(name));
    }
    Ok(None)
}
