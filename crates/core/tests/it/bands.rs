//! Banded rendering (B1): the picture drawn a band of rows at a time. The one promise is that the bands put together are the
//! picture `render` makes, bit for bit, whatever the height of a band. The oracle is `render` itself, over scenes of every kind
//! of layer at several band heights and at sizes whose long side is odd and even.

use std::ops::Range;
use std::sync::Arc;

use bullterrier_scene::{
    BAND_PIXELS, Blend, Deg, Error, Fit, Layer, LayerKind, Mask, Place, Raster, Scene, Text, Use, Values,
    check_by_position,
};
use serde_json::{Value, json};

use crate::common::counting::Counting;
use crate::common::{Outcome, made, rectangle, scene, varied};
use crate::saved::{Files, rich};

/// The first place where the picture and `bands` differ, as `(row, column, channel)`, or `None`.
fn first_difference(whole: &Raster, bands: &[f32]) -> Option<(usize, usize, usize)> {
    assert_eq!(whole.pixels().len(), bands.len(), "the bands cover the picture exactly");
    let at = whole
        .pixels()
        .iter()
        .zip(bands)
        .position(|(left, right)| left.to_bits() != right.to_bits())?;
    let width = whole.width();
    Some((at / 4 / width, at / 4 % width, at % 4))
}

/// The heights a band takes in a picture of `rows` rows: one, a few, a prime near an eighth, half, all and some that divide
/// nothing.
fn heights(rows: usize) -> Vec<usize> {
    let prime_near = |target: usize| {
        (target.max(2)..=target.max(2) * 2)
            .find(|n| (2..*n).all(|d| n % d != 0))
            .unwrap_or(1)
    };
    let mut all = vec![
        1,
        7,
        prime_near(rows / 8),
        rows / 2,
        rows,
        rows * 2 / 3 + 1,
        rows.saturating_sub(1),
    ];
    all.retain(|height| (1..=rows).contains(height));
    all.sort_unstable();
    all.dedup();
    all
}

/// Every band height draws the picture `render` draws, the bands in order and covering the rows exactly.
fn bands_make_the_whole(scene: &Scene, size: u32) -> Outcome {
    let whole = scene.render(size)?;
    assert_eq!(scene.picture_size(size)?, (whole.width(), whole.height()));
    for band_rows in heights(whole.height()) {
        let (mut next, mut found) = (0, Vec::new());
        scene.render_bands(size, band_rows, |rows: Range<usize>, band: &Raster| {
            assert_eq!(rows.start, next, "a band starts where the last one ended");
            assert!(rows.start < rows.end && rows.len() <= band_rows);
            assert_eq!((band.width(), band.height()), (whole.width(), rows.len()));
            found.extend_from_slice(band.pixels());
            next = rows.end;
            Ok(())
        })?;
        assert_eq!(next, whole.height(), "the bands reach the last row");
        assert_eq!(
            first_difference(&whole, &found),
            None,
            "bands of {band_rows} rows at size {size}: first difference (row, column, channel)"
        );
    }
    Ok(())
}

/// Each size, odd and even on the long side.
fn at_sizes(scene: &Scene, sizes: &[u32]) -> Outcome {
    sizes.iter().try_for_each(|&size| bands_make_the_whole(scene, size))
}

/// Every layer a feature that cuts across a band: a turn, a skew, a perspective, a feathered mask by a drawing and by a layer
/// that is masked in its turn, a group put at a fraction of a pixel, a reference, a blend and a light layer, type, a drawing at
/// a fractional phase and a plugin kind.
fn geometry() -> Result<Scene, Error> {
    let shape = |x: f64, y: f64, w: f64, h: f64, color: &str| rectangle((60, 44), (x, y), (x + w, y + h), color);
    let badge = scene(
        18,
        12,
        [
            Layer::fill("#336699")?.named("plate")?.opacity(0.7)?,
            Layer::fill("#ffcc00")?
                .named("bar")?
                .fit([2.0, 4.0, 14.0, 3.0], Fit::Stretch, [0.5, 0.5])?,
        ],
    )?;
    let ramp = made(16, 12, |x, y| {
        let level = |n: usize, d: f32| f32::from(u8::try_from(n).unwrap_or(0)) / d;
        [level(x, 16.0), level(y, 12.0), level((x + y) % 7, 7.0), 1.0]
    })?;
    let hole = Layer::fill("#ffffff")?
        .fit([6.0, 4.0, 48.0, 36.0], Fit::Stretch, [0.5, 0.5])?
        .mask(Mask::new(Layer::drawing(&shape(8.0, 2.0, 40.0, 40.0, "#ffffff"))?).feather(1.7))?;
    scene(
        60,
        44,
        [
            Layer::fill("#1a2a40")?.named("ground")?,
            Layer::picture(ramp.clone())
                .named("turned")?
                .fit([4.0, 4.0, 30.0, 20.0], Fit::Cover, [0.5, 0.5])?
                .place(Place::new().rotate(Deg(17.0)).dx(7.3).dy(2.1))?,
            Layer::picture(ramp.clone())
                .named("skewed")?
                .fit([30.0, 20.0, 28.0, 20.0], Fit::Stretch, [0.5, 0.5])?
                .place(Place::new().skew_x(Deg(25.0)).aspect(0.8))?
                .opacity(0.9)?,
            Layer::picture(ramp)
                .named("slanted")?
                .perspective([[2.0, 24.0], [30.0, 28.0], [34.0, 42.0], [0.0, 40.0]])?
                .opacity(0.8)?,
            Layer::picture(varied(10, 10)?)
                .named("feathered")?
                .fit([10.0, 8.0, 40.0, 30.0], Fit::Cover, [0.5, 0.5])?
                .mask(Mask::new(Layer::drawing(&shape(12.0, 6.0, 30.0, 26.0, "#ffffff"))?).feather(3.2))?
                .mask(Mask::new(hole).using(Use::Luminance).feather(0.9).invert())?
                .opacity(0.85)?,
            Layer::scene(badge)
                .named("badge")?
                .place(Place::new().dx(20.25).dy(9.4).rotate(Deg(8.0)))?,
            Layer::of("badge")
                .named("copy")?
                .place(Place::new().dx(-3.5).dy(28.5).scale(1.4))?
                .blend(Blend::Multiply),
            Layer::drawing(&shape(1.0, 1.0, 20.0, 12.0, "#ff3366"))?
                .named("shape")?
                .place(Place::new().dx(31.7).dy(0.3).scale(1.3))?,
            Layer::text(
                "Band",
                &Text::new("bricolage_grotesque").color("#ffffff").option("size", 11.0),
            )?
            .named("word")?
            .place(Place::new().dx(3.0).dy(20.0))?,
            Layer::of("shape")
                .named("halo")?
                .look(r#"{"steps": [{"effect": "bloom", "params": {"radius": 3.0, "threshold": 0.2}}]}"#)?
                .as_light(true)
                .blend(Blend::Screen)
                .place(Place::new().dx(2.0).dy(14.0))?,
            Layer::make(Counting::new(0.3))
                .named("plugin")?
                .fit([36.0, 30.0, 20.0, 12.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.6)?,
        ],
    )
}

#[test]
fn bands_of_every_height_make_the_picture_a_scene_of_every_kind_of_layer_makes() -> Outcome {
    at_sizes(&geometry()?, &[60, 61, 97, 120])
}

#[test]
fn bands_of_every_height_make_the_poster_of_the_cache_tests() -> Outcome {
    at_sizes(&crate::cache::poster()?, &[60, 61, 95])
}

#[test]
fn bands_of_every_height_make_the_scene_with_every_part_of_the_document() -> Outcome {
    let files = Files::new()?;
    let filled = rich(&files)?.with_values(
        &Values::new()
            .set("hole.picture", files.path("photo.png"))
            .set("photo.map.later", files.path("matte.png")),
    )?;
    at_sizes(&filled, &[24, 25, 48, 71])
}

#[test]
fn bands_of_every_height_make_the_cover_template() -> Outcome {
    let files = Files::new()?;
    let template = crate::cover_template::build(&files.path("photo.png"))?;
    at_sizes(&template, &[160, 161])
}

#[test]
fn bands_of_every_height_make_the_scene_of_every_kind_of_parameter() -> Outcome {
    at_sizes(&crate::parameters::drawable()?, &[40, 41, 77])
}

/// A picture one row tall, one column wide, and a band as tall as the picture.
#[test]
fn a_picture_of_one_row_or_one_column_is_made_by_bands() -> Outcome {
    let one = scene(8, 1, [Layer::picture(varied(4, 1)?)])?;
    at_sizes(&one, &[8, 5, 1])?;
    let tall = scene(1, 9, [Layer::picture(varied(1, 3)?)])?;
    at_sizes(&tall, &[9, 4])
}

/// What a sink that refuses does: the render stops at that band and the refusal is the render's, not another error.
#[test]
fn a_refusal_of_the_sink_ends_the_render_and_is_returned() -> Outcome {
    let scene_ = geometry()?;
    let mut seen = 0;
    let refused = scene_.render_bands(60, 10, |_, _| {
        seen += 1;
        if seen == 2 {
            Err(Error::Write {
                path: "x.png".to_owned(),
                message: "no room".to_owned(),
            })
        } else {
            Ok(())
        }
    });
    assert!(
        matches!(refused, Err(Error::Write { ref path, .. }) if path == "x.png"),
        "{refused:?}"
    );
    assert_eq!(seen, 2, "no band was drawn after the refusal");
    Ok(())
}

/// A band of no rows, of more rows than the picture has, and a size outside its range are refused, naming the argument.
#[test]
fn a_band_that_is_no_band_and_a_size_that_is_none_are_refused_by_name() -> Outcome {
    let scene_ = scene(8, 4, [Layer::fill("#808080")?])?;
    let sink = |_: Range<usize>, _: &Raster| Ok(());
    for (size, band_rows, named) in [
        (8, 0, "band_rows"),
        (8, 5, "band_rows"),
        (0, 1, "size"),
        (65_001, 1, "size"),
    ] {
        let refused = scene_.render_bands(size, band_rows, sink);
        assert!(
            matches!(&refused, Err(Error::Invalid { address, .. }) if address == named),
            "size {size}, {band_rows} rows: {refused:?}"
        );
    }
    scene_.render_bands(8, 4, sink)?;
    Ok(())
}

/// A layer of a plugin kind with no look is asked once for its rectangle, however many bands the picture is drawn in: it is
/// drawn whole and kept, and each band is cut from it.
#[test]
fn a_plugin_layer_is_drawn_once_however_many_bands_there_are() -> Outcome {
    let kind = Counting::new(0.5);
    let planted = scene(40, 30, [Layer::make(kind.clone())])?;
    planted.render_bands(40, 3, |_, _| Ok(()))?;
    assert_eq!(kind.draws(), 1);
    Ok(())
}

/// A kind that says its pixels depend on their place alone is asked for the rows a band reads, not the whole rectangle: each band
/// is made of a part of it, and the bands put together are the picture drawn whole, at a scale that is whole and at ones that are not
/// (where the pixels do not divide the canvas into whole design pixels).
#[test]
fn a_kind_that_draws_by_position_is_asked_for_the_rows_a_band_reads() -> Outcome {
    let kind = Counting::positional(0.5);
    let planted = scene(40, 30, [Layer::make(kind.clone())])?;
    planted.render_bands(40, 3, |_, _| Ok(()))?;
    let questions = kind.questions();
    assert!(questions.len() >= 10, "a question for each band: {}", questions.len());
    assert!(
        questions.iter().all(|(area, _)| area[3] < 30.0),
        "no question is for the whole rectangle: {questions:?}"
    );
    for size in [40, 37, 33] {
        bands_make_the_whole(&scene(40, 30, [Layer::make(Counting::positional(0.5))])?, size)?;
    }
    Ok(())
}

/// A looked layer of such a kind is drawn by band and margin like a drawing.
#[test]
fn a_looked_kind_that_draws_by_position_is_drawn_by_band() -> Outcome {
    let vignette = r#"{"steps": [{"effect": "vignette", "params": {"amount": 0.9}}]}"#;
    let kind = Counting::positional(0.5);
    let planted = scene(40, 30, [Layer::make(kind.clone()).look(vignette)?])?;
    bands_make_the_whole(&planted, 40)?;
    let fresh = Counting::positional(0.5);
    scene(40, 30, [Layer::make(fresh.clone()).look(vignette)?])?.render_bands(40, 3, |_, _| Ok(()))?;
    assert!(fresh.draws() >= 10 && fresh.questions().iter().all(|(area, _)| area[3] < 30.0));
    Ok(())
}

/// The check a kind's tests run before it says it draws by position: it passes a kind whose pixels are where they are, and names one
/// whose pixels depend on the rectangle asked.
#[test]
fn the_check_of_a_kind_that_draws_by_position_passes_one_that_does_and_names_one_that_does_not() -> Outcome {
    let (area, scale) = ([3.0, 2.0, 40.0, 30.0], [0.925, 0.925]);
    check_by_position(&*Counting::positional(0.5), area, scale)?;
    // pixels at a pitch of one over the scale, not the rectangle's even division: a rectangle of part of the rows is another place
    let refused = check_by_position(&*Counting::new(0.5), area, scale);
    assert!(
        matches!(refused, Err(Error::Invalid { ref address, .. }) if address == "by_position"),
        "{refused:?}"
    );
    Ok(())
}

/// A scene whose picture has more pixels than a band holds (so a file written from it is written in more than one band): a
/// ground that stops short of the bottom, a photo and a translucent bar, 4,100 by 4,100 pixels at size 4,100.
pub(crate) fn past_one_band() -> Result<Scene, Error> {
    let scene_ = scene(
        4100,
        4100,
        [
            Layer::fill("#204080")?
                .named("ground")?
                .fit([0.0, 0.0, 4100.0, 3600.0], Fit::Stretch, [0.5, 0.5])?,
            Layer::picture(varied(12, 8)?)
                .named("photo")?
                .fit([100.0, 100.0, 3000.0, 2000.0], Fit::Cover, [0.5, 0.5])?
                .place(Place::new().rotate(Deg(3.0)))?,
            Layer::fill("#ff000080")?
                .named("bar")?
                .fit([0.0, 3000.0, 4100.0, 700.0], Fit::Stretch, [0.5, 0.5])?,
        ],
    )?;
    let (columns, rows) = scene_.picture_size(4100)?;
    assert!(columns * rows > BAND_PIXELS, "the picture is more than one band");
    Ok(scene_)
}

/// What a PNG file holds, as image reads it back: width, height and the levels.
fn read_back(path: &std::path::Path) -> Result<(usize, usize, Vec<f32>), Box<dyn std::error::Error>> {
    let image = bullterrier_image::read(path)?;
    Ok((image.width(), image.height(), image.into_values()))
}

/// The PNG streamed band by band holds the pixels the whole picture written by `Raster::write` holds, to the bit once read
/// back (the same 8-bit levels, rounded the same way); the two files differ only in how they say the colour is sRGB.
#[test]
fn a_png_streamed_by_bands_holds_the_pixels_of_the_picture_written_whole() -> Outcome {
    let folder = tempfile::tempdir()?;
    let scene_ = past_one_band()?;
    let (streamed, whole) = (folder.path().join("streamed.png"), folder.path().join("whole.png"));
    scene_.render_to_png(4100, &streamed)?;
    scene_.render(4100)?.write(&whole)?;
    let (a, b) = (read_back(&streamed)?, read_back(&whole)?);
    assert_eq!((a.0, a.1), (b.0, b.1));
    assert_eq!(a.2.iter().zip(&b.2).position(|(x, y)| x.to_bits() != y.to_bits()), None);
    assert_eq!(a.2.len(), b.2.len());
    // the folder holds the two files and no temporary one
    assert_eq!(std::fs::read_dir(folder.path())?.count(), 2);
    Ok(())
}

/// A kind that cannot draw.
struct Broken;

impl LayerKind for Broken {
    fn name(&self) -> &'static str {
        "broken"
    }

    fn options(&self) -> Value {
        json!({})
    }

    fn draw(&self, _: [f64; 4], _: [f64; 2]) -> Result<Raster, Error> {
        Err(Error::Invalid {
            address: "broken".to_owned(),
            message: "this kind draws nothing".to_owned(),
        })
    }
}

/// A render that fails leaves no file, and one that was there stays as it was, whichever file it writes.
#[test]
fn a_render_that_fails_leaves_no_file_and_the_old_file_as_it_was() -> Outcome {
    let folder = tempfile::tempdir()?;
    let scene_ = scene(40, 30, [Layer::fill("#336699")?, Layer::make(Arc::new(Broken))])?;
    let options = bullterrier_scene::PdfOptions::new(300.0)?;
    let (png, pdf) = (folder.path().join("a.png"), folder.path().join("a.pdf"));
    assert!(matches!(scene_.render_to_png(40, &png), Err(Error::Invalid { .. })));
    assert!(matches!(
        scene_.render_to_pdf(40, &pdf, &options),
        Err(Error::Invalid { .. })
    ));
    assert_eq!(
        std::fs::read_dir(folder.path())?.count(),
        0,
        "no file, and no temporary one"
    );
    std::fs::write(&png, b"old")?;
    assert!(scene_.render_to_png(40, &png).is_err());
    assert_eq!(std::fs::read(&png)?, b"old");
    // a folder that is not there
    let missing = folder.path().join("nowhere").join("b.png");
    assert!(matches!(scene_.render_to_png(40, &missing), Err(Error::Write { .. })));
    Ok(())
}

/// A group zoomed a thousand times is 100,000 by 100,000 pixels where it is drawn: `render` refuses it by the limit of 2^27
/// pixels on a layer, and drawn by bands only the rows a band reads are drawn, so the zoom shows what lies at the centre. The
/// group is a plate of two colours that meet at its middle row, which the zoom keeps at the middle of the picture.
#[test]
fn a_group_zoomed_past_the_limit_of_a_layer_is_drawn_by_the_rows_a_band_reads() -> Outcome {
    let plate = scene(
        100,
        100,
        [
            Layer::fill("#c04020")?.named("top")?,
            Layer::fill("#2040c0")?
                .named("bottom")?
                .fit([0.0, 50.0, 100.0, 50.0], Fit::Stretch, [0.5, 0.5])?,
        ],
    )?;
    let zoomed = scene(100, 100, [Layer::scene(plate).place(Place::new().scale(1000.0))?])?;
    assert!(
        matches!(zoomed.render(100), Err(Error::Invalid { ref address, .. }) if address.ends_with("size")),
        "a whole render is refused by the limit of a layer"
    );
    let mut rows = Vec::new();
    zoomed.render_bands(100, 10, |range, band| {
        let level = |row: usize| band.pixels()[row * band.width() * 4..][..4].to_vec();
        rows.push((range.start, level(0), level(band.height() - 1)));
        Ok(())
    })?;
    let colour = |hex: u32| [16, 8, 0].map(|shift| f32::from(u8::try_from((hex >> shift) & 255).unwrap_or(0)) / 255.0);
    let (red, blue) = (colour(0xc0_40_20), colour(0x20_40_c0));
    for (first, top, bottom) in rows {
        let want = if first < 50 { red } else { blue };
        for found in [top, bottom] {
            assert!(
                found[..3].iter().zip(want).all(|(got, want)| (got - want).abs() < 1e-6)
                    && (found[3] - 1.0).abs() < 1e-6,
                "band at row {first}: {found:?}, wanted {want:?}"
            );
        }
    }
    Ok(())
}

/// A drawing zoomed 500 times is 50,000 by 50,000 pixels where it is drawn: `render` refuses it by the limit of 2^27 pixels on
/// a layer, and drawn by bands only the rows a band reads are drawn (vector's window), so the zoom shows what lies at the
/// centre. The drawing's top half is red; the picture's middle row is where it ends.
#[test]
fn a_drawing_zoomed_past_the_limit_of_a_layer_is_drawn_by_the_rows_a_band_reads() -> Outcome {
    let half = rectangle((100, 100), (0.0, 0.0), (100.0, 50.0), "#c04020");
    let zoomed = scene(100, 100, [Layer::drawing(&half)?.place(Place::new().scale(500.0))?])?;
    assert!(
        matches!(zoomed.render(100), Err(Error::Invalid { ref address, .. }) if address.ends_with("size")),
        "a whole render is refused by the limit of a layer"
    );
    let mut seen = 0;
    zoomed.render_bands(100, 10, |range, band| {
        let level = |row: usize| -> [f32; 4] {
            band.pixels()[row * band.width() * 4..][..4]
                .try_into()
                .unwrap_or([f32::NAN; 4])
        };
        let (top, bottom) = (level(0), level(band.height() - 1));
        for (row, found) in [(range.start, top), (range.end - 1, bottom)] {
            let [red, green, blue, alpha] = found;
            if row < 50 {
                assert!(
                    (red - f32::from(0xc0_u8) / 255.0).abs() < 1e-6 && alpha > 0.999,
                    "row {row}: {found:?}"
                );
                assert!(
                    (green - f32::from(0x40_u8) / 255.0).abs() < 1e-6
                        && (blue - f32::from(0x20_u8) / 255.0).abs() < 1e-6
                );
            } else {
                assert!(alpha < 1e-6, "row {row}: {found:?}");
            }
        }
        seen += 1;
        Ok(())
    })?;
    assert_eq!(seen, 10);
    Ok(())
}

/// Where vector refuses a window (a layer more than 65,000 pixels wide), the whole render of the layer is refused too, so the
/// bands fail the way the whole render does, naming the window, and draw nothing wrong.
#[test]
fn a_drawing_vector_cannot_window_is_refused_by_bands_as_by_the_whole_render() -> Outcome {
    let half = rectangle((100, 100), (0.0, 0.0), (100.0, 50.0), "#c04020");
    let wide = scene(100, 100, [Layer::drawing(&half)?.place(Place::new().scale(1000.0))?])?;
    assert!(matches!(wide.render(100), Err(Error::Invalid { .. })));
    let refused = wide.render_bands(100, 10, |_, _| Ok(()));
    assert!(
        matches!(refused, Err(Error::Invalid { ref message, .. }) if message.contains("window")),
        "{refused:?}"
    );
    Ok(())
}

/// A recipe of one step as a look.
fn one_step(effect: &str, params: &str) -> String {
    format!(r#"{{"steps": [{{"effect": "{effect}", "params": {params}}}]}}"#)
}

/// A looked drawing, a group of one and a picture (enlarged, and shrunk past 2x) that fill the canvas, drawn by bands, equal the
/// whole render for every recipe image can run on a band (a place by the picture: vignette; a position-hashed grain; a blur and a
/// chain of looks whose margins add up), and for those it cannot (a blur summed by a Fourier transform, a statistic of the whole
/// picture): drawn whole and cut. The drawing stops short of the frame's edges, so the margin rows hold other content than the band's.
#[test]
fn looked_layers_drawn_by_bands_make_the_picture_they_make_whole() -> Outcome {
    let shape = rectangle((100, 100), (12.0, 22.0), (71.0, 93.0), "#c04020");
    let cases = [
        (
            "vignette",
            vec![one_step("vignette", r#"{"amount": 0.9, "radius": 0.2}"#)],
        ),
        ("grain", vec![one_step("grain", r#"{"amount": 0.6, "size": 1.5}"#)]),
        ("small blur", vec![one_step("gaussian_blur", r#"{"radius": 8.0}"#)]),
        (
            "blur reaching 16 pixels",
            vec![one_step("gaussian_blur", r#"{"radius": 24.0}"#)],
        ),
        (
            "chain whose margins add",
            vec![
                one_step("gaussian_blur", r#"{"radius": 6.0}"#),
                one_step("grain", r#"{"amount": 0.4}"#),
                one_step("vignette", r#"{"amount": 0.5}"#),
                one_step("gaussian_blur", r#"{"radius": 6.0}"#),
            ],
        ),
        (
            "blur summed by a Fourier transform",
            vec![one_step("gaussian_blur", r#"{"radius": 300.0}"#)],
        ),
        ("statistic of the whole picture", vec![one_step("auto_levels", "{}")]),
    ];
    let fill = [100.0, 100.0];
    let fitted = |raster: Raster| -> Result<Layer, Error> {
        Layer::picture(raster).fit([0.0, 0.0, fill[0], fill[1]], Fit::Cover, [0.5, 0.5])
    };
    for (name, looks) in cases {
        let bases = [
            ("drawing", Layer::drawing(&shape)?),
            ("group", Layer::scene(scene(100, 100, [Layer::drawing(&shape)?])?)),
            ("enlarged picture", fitted(varied(40, 30)?)?),
            ("shrunk picture", fitted(varied(320, 240)?)?),
        ];
        for (kind, mut layer) in bases {
            for look in &looks {
                layer = layer.look(look)?;
            }
            let looked = scene(100, 100, [Layer::fill("#204080")?, layer])?;
            at_sizes(&looked, &[100, 97]).map_err(|error| format!("{kind} with {name}: {error}"))?;
        }
    }
    Ok(())
}
