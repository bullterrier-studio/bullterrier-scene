//! Plates: one plane of ink coverage for each ink a scene prints in.

use crate::blend::Blend;
use crate::layer::Base;
use crate::{Error, Layer, Scene};

/// The ink coverage of a picture, one plane for each ink, at the size a scene was drawn at.
#[derive(Clone, PartialEq)]
pub struct Plates {
    columns: usize,
    rows: usize,
    planes: Vec<(String, Vec<f32>)>,
}

impl std::fmt::Debug for Plates {
    /// The size and the inks, never the values.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Plates {{ columns: {}, rows: {}, inks: {:?} }}",
            self.columns,
            self.rows,
            self.names()
        )
    }
}

impl Plates {
    /// The number of columns of every plane.
    #[must_use]
    pub fn width(&self) -> usize {
        self.columns
    }

    /// The number of rows of every plane.
    #[must_use]
    pub fn height(&self) -> usize {
        self.rows
    }

    /// The inks, in the order the layers name them.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.planes.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// How much of ink `name` lies on each pixel, 0 to 1: `width * height` values, row after row from the top.
    #[must_use]
    pub fn plane(&self, name: &str) -> Option<&[f32]> {
        self.planes
            .iter()
            .find(|(found, _)| found == name)
            .map(|(_, values)| values.as_slice())
    }
}

/// The most values the planes of plates hold together: 2^27 floats, 512 MiB.
const MAX_VALUES: usize = 1 << 27;

/// The inks the layers of `drawn` name, in the order they first appear.
fn ink_names(drawn: &[Layer]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in drawn.iter().flat_map(|layer| &layer.join.inks) {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names
}

/// `layer` as ink `ink` draws it, or `None` for a layer that does not print in it: the layer with its last look making the
/// plane of that ink (or the shape of its alpha), joined normally.
///
/// # Errors
/// [`Error::Invalid`] naming `<layer>.inks` for a layer that is light, or that cannot hand its inks on.
fn for_ink(layer: &Layer, place: usize, ink: &str) -> Result<Option<Layer>, Error> {
    let Some(index) = layer.join.inks.iter().position(|name| name == ink) else {
        return Ok(None);
    };
    let label = layer
        .name
        .clone()
        .unwrap_or_else(|| format!("layer{}", place + 1));
    let refused = |error: Error| error.in_layer(&label);
    if layer.join.light {
        return Err(refused(Error::invalid(
            "inks",
            "a light layer makes its own alpha from its colour, so it has no shape to print",
        )));
    }
    let mut plate = layer.clone();
    let Base::Body(body) = &mut plate.base else {
        return Err(refused(Error::invalid("inks", "the layer was not resolved by a scene")));
    };
    body.finish = body
        .finish
        .for_plate(index, layer.join.inks.len())
        .map_err(refused)?;
    plate.join.blend = Blend::Normal;
    Ok(Some(plate))
}

/// The plates of `scene` at `size`.
///
/// # Errors
/// As [`Scene::plates`].
pub(crate) fn make(scene: &Scene, size: i128) -> Result<Plates, Error> {
    let (columns, rows) = scene.picture_size(size)?;
    let names = ink_names(scene.drawn());
    if names.is_empty() {
        return Err(Error::invalid(
            "inks",
            "no layer names an ink: `Layer::inks` says what each layer prints in",
        ));
    }
    if columns.saturating_mul(rows).saturating_mul(names.len()) > MAX_VALUES {
        return Err(Error::invalid(
            "size",
            format!(
                "{columns} x {rows} pixels in {} plates are more than the {MAX_VALUES} values plates hold",
                names.len()
            ),
        ));
    }
    let mut planes = Vec::with_capacity(names.len());
    for ink in names {
        let layers = scene
            .drawn()
            .iter()
            .enumerate()
            .filter_map(|(place, layer)| for_ink(layer, place, ink).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let picture = scene.drawing(layers).render(size)?;
        let coverage = picture.pixels().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();
        planes.push((ink.to_owned(), coverage));
    }
    Ok(Plates { columns, rows, planes })
}
