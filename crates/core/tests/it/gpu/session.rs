//! What a context keeps and counts: the same bytes twice, a changed value against a cold render, what crosses to the device,
//! the device cache's budget, two threads on one context, and the refusals.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bullterrier_scene::{Error, Fit, Gpu, Layer, LayerKind, Raster, Scene, Values};
use pollster::block_on;
use serde_json::{Value, json};

use super::{fresh, gpu, rendered};
use crate::common::{Outcome, rectangle, scene};

/// A picture of one seeded grey ramp, so that two differ.
fn ramp(width: usize, height: usize, seed: usize) -> Result<Raster, Error> {
    let pixels = (0..width * height)
        .flat_map(|at| {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let level = ((at * 7 + seed * 13) % 101) as f32 / 100.0;
            [level, 1.0 - level, (level * 3.0).fract(), 1.0]
        })
        .collect();
    Raster::rgba(width, height, pixels)
}

fn picture(seed: usize, name: &str) -> Result<Layer, Error> {
    Layer::picture(ramp(30, 20, seed)?)
        .fit([0.0, 0.0, 60.0, 40.0], Fit::Cover, [0.5, 0.5])?
        .named(name)
}

/// Whether two pictures are the same bit for bit: the length, then the first index that differs (a failure names it, and no
/// buffer is printed).
fn same_bits(left: &Raster, right: &Raster) -> Option<usize> {
    assert_eq!((left.width(), left.height()), (right.width(), right.height()));
    left.pixels()
        .iter()
        .zip(right.pixels())
        .position(|(a, b)| a.to_bits() != b.to_bits())
}

/// Three pictures, a fill and a drawing.
fn poster(drawing: &str) -> Result<Scene, Error> {
    scene(
        60,
        40,
        [
            Layer::fill("#203040")?.named("ground")?,
            picture(1, "one")?,
            picture(2, "two")?.opacity(0.6)?,
            picture(3, "three")?.opacity(0.4)?,
            Layer::drawing(drawing)?.named("mark")?,
        ],
    )
}

fn mark(colour: &str) -> String {
    rectangle((60, 40), (10.0, 5.0), (40.0, 30.0), colour)
}

#[test]
fn the_same_context_and_scene_give_the_same_bytes_twice() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let poster = poster(&mark("#ff0000"))?;
    let (first, second) = (rendered(&gpu, &poster, 60)?, rendered(&gpu, &poster, 60)?);
    assert_eq!(same_bits(&first, &second), None);
    Ok(())
}

#[test]
fn a_changed_value_gives_the_bytes_of_a_cold_render_of_the_changed_scene() -> Outcome {
    let (Some(warm), Some(cold)) = (fresh(), fresh()) else {
        return Ok(());
    };
    let values = Values::new().set("two.opacity", 0.9).set("three.opacity", 0.1);
    let used = poster(&mark("#ff0000"))?;
    rendered(&warm, &used, 60)?;
    let changed = rendered(&warm, &used.with_values(&values)?, 60)?;
    // a scene made afresh, on a context that has drawn nothing
    let cold_scene = poster(&mark("#ff0000"))?.with_values(&values)?;
    let cold = rendered(&cold, &cold_scene, 60)?;
    assert_eq!(same_bits(&changed, &cold), None);
    Ok(())
}

#[test]
fn sources_cross_to_the_device_once_and_the_picture_comes_back_once() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let poster = poster(&mark("#ff0000"))?;
    let resident = block_on(gpu.render(&poster, 60))?;
    let sent = gpu.transfers();
    // the fill, three pictures and the drawing: five sources, nothing read back
    assert_eq!((sent.uploads, sent.downloads), (5, 0), "{sent:?}");
    block_on(gpu.render(&poster, 60))?;
    assert_eq!(gpu.transfers().uploads, 5, "the same scene again sends nothing");
    // a layer that only fades sends nothing; a drawing that changes sends one
    block_on(gpu.render(&poster.with_values(&Values::new().set("one.opacity", 0.5))?, 60))?;
    assert_eq!(gpu.transfers().uploads, 5);
    block_on(gpu.render(&self::poster(&mark("#00ff00"))?, 60))?;
    assert_eq!(gpu.transfers().uploads, 6, "the new drawing, once");
    let picture = block_on(gpu.download(&resident))?;
    assert_eq!(gpu.transfers().downloads, 1);
    assert_eq!((picture.width(), picture.height()), (60, 40));
    Ok(())
}

#[test]
fn the_device_cache_keeps_what_its_budget_holds_and_nothing_mixes_with_the_cpus() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let poster = poster(&mark("#ff0000"))?;
    let cpu = poster.render(60)?;
    // a budget that holds one of the three pictures (30 x 20 x 16 bytes = 9,600) and the 1 pixel fill, not the five sources
    gpu.set_cache_budget(10_000);
    assert_eq!(gpu.cache_budget(), 10_000);
    let held = rendered(&gpu, &poster, 60)?;
    let after_first = gpu.transfers().uploads;
    rendered(&gpu, &poster, 60)?;
    assert!(
        gpu.transfers().uploads > after_first,
        "the budget holds less than the scene's sources: the second render sends some again"
    );
    // the CPU's picture is untouched by the GPU's renders, and the GPU's by the CPU's cache
    assert_eq!(same_bits(&cpu, &poster.render(60)?), None);
    assert!(super::largest_gap(&cpu, &held).0 < 1e-3);
    gpu.set_cache_budget(bullterrier_scene::DEFAULT_CACHE_BYTES);
    rendered(&gpu, &poster, 60)?;
    let settled = gpu.transfers().uploads;
    rendered(&gpu, &poster, 60)?;
    assert_eq!(gpu.transfers().uploads, settled, "with room for them the sources stay");
    Ok(())
}

#[test]
fn two_threads_render_different_scenes_on_one_context() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let (one, other) = (
        poster(&mark("#ff0000"))?,
        scene(60, 40, [picture(9, "nine")?, Layer::fill("#80ff80")?.opacity(0.5)?])?,
    );
    let alone = (rendered(&gpu, &one, 60)?, rendered(&gpu, &other, 60)?);
    let together = std::thread::scope(|threads| {
        let first = threads.spawn(|| {
            (0..4)
                .map(|_| rendered(&gpu, &one, 60))
                .collect::<Result<Vec<_>, Error>>()
        });
        let second = threads.spawn(|| {
            (0..4)
                .map(|_| rendered(&gpu, &other, 60))
                .collect::<Result<Vec<_>, Error>>()
        });
        (first.join(), second.join())
    });
    let (Ok(first), Ok(second)) = together else {
        return Err("a render thread panicked".into());
    };
    for picture in first? {
        assert_eq!(same_bits(&picture, &alone.0), None);
    }
    for picture in second? {
        assert_eq!(same_bits(&picture, &alone.1), None);
    }
    Ok(())
}

/// A caller's device whose storage buffers hold 64 KiB: 4,096 pixels of 16 bytes.
fn small_device() -> Result<Gpu, Box<dyn std::error::Error>> {
    let (device, queue) = block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: 64 * 1024,
            ..wgpu::Limits::downlevel_defaults()
        };
        Ok::<_, Box<dyn std::error::Error>>(
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_limits,
                    ..Default::default()
                })
                .await?,
        )
    })?;
    Ok(Gpu::new(device, queue)?)
}

#[test]
fn a_size_beyond_the_device_is_a_gpu_error_naming_the_cpu_and_a_bad_size_is_the_scenes_own() -> Outcome {
    let Some(_alone) = gpu() else { return Ok(()) };
    let small = small_device()?;
    let flat = scene(65, 65, [Layer::fill("#336699")?])?;
    // 64 x 64 fits; 65 x 65 does not
    assert!(block_on(small.render(&scene(64, 64, [Layer::fill("#336699")?])?, 64)).is_ok());
    let Err(error) = block_on(small.render(&flat, 65)) else {
        return Err("a picture beyond the device was rendered".into());
    };
    assert!(matches!(error, Error::Gpu { .. }), "{error:?}");
    assert!(
        error.to_string().contains("65 x 65") && error.to_string().contains("CPU"),
        "{error}"
    );
    // the CPU's own refusal of a size is the same on the GPU: it is the caller's, not the machine's
    let Err(error) = block_on(small.render(&flat, 0)) else {
        return Err("size 0 was rendered".into());
    };
    assert!(matches!(error, Error::Invalid { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_looked_picture_is_sent_once_and_nothing_comes_back_until_the_end() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let grade = r#"{"steps": [{"effect": "grade", "params": {"contrast": 1.4}}, {"effect": "levels", "params": {"gamma": 1.3}}]}"#;
    let looked = scene(
        60,
        40,
        [
            Layer::fill("#102030")?,
            picture(4, "photo")?.look(grade)?,
            picture(5, "other")?.opacity(0.5)?,
        ],
    )?;
    let resident = block_on(gpu.render(&looked, 60))?;
    let sent = gpu.transfers();
    // the fill, the looked picture (one upload, the two looks chained from it) and the plain one
    assert_eq!((sent.uploads, sent.downloads), (3, 0), "{sent:?}");
    block_on(gpu.render(&looked, 60))?;
    assert_eq!(gpu.transfers().uploads, 3, "the looked picture is kept on the device");
    // a slider on the other layer sends nothing and runs no look again
    block_on(gpu.render(&looked.with_values(&Values::new().set("other.opacity", 0.9))?, 60))?;
    assert_eq!(gpu.transfers().uploads, 3);
    block_on(gpu.download(&resident))?;
    assert_eq!(gpu.transfers().downloads, 1);
    Ok(())
}

/// A plugin kind that, when asked to draw, says so and then waits for the go-ahead of another render on the same context.
struct Waits {
    entered: Mutex<Sender<()>>,
    go: Mutex<Receiver<()>>,
}

impl LayerKind for Waits {
    fn name(&self) -> &'static str {
        "waits"
    }

    fn options(&self) -> Value {
        json!({})
    }

    fn draw(&self, area: [f64; 4], _scale: [f64; 2]) -> Result<Raster, Error> {
        let stuck = |what: &str| Error::Gpu {
            message: format!("the plugin kind {what}: another render could not run while this one drew"),
        };
        self.entered
            .lock()
            .map_err(|_| stuck("lost its channel"))?
            .send(())
            .map_err(|_| stuck("lost its channel"))?;
        self.go
            .lock()
            .map_err(|_| stuck("lost its channel"))?
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| stuck("timed out"))?;
        Raster::rgba(
            area[2] as usize,
            area[3] as usize,
            vec![0.5; area[2] as usize * area[3] as usize * 4],
        )
    }
}

/// What a source draws on the CPU (a plugin kind may take as long as it likes) holds no lock of the context: a render of
/// another scene on the same context runs while it draws.
#[test]
fn a_render_runs_while_another_on_the_same_context_is_drawing_a_plugin_layer() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let (entered, entering) = channel();
    let (go, going) = channel();
    let slow = scene(
        60,
        40,
        [Layer::make(Arc::new(Waits {
            entered: Mutex::new(entered),
            go: Mutex::new(going),
        }))],
    )?;
    let quick = scene(60, 40, [Layer::fill("#203040")?])?;
    let (slow_result, quick_result) = std::thread::scope(|threads| {
        let slow_render = threads.spawn(|| rendered(&gpu, &slow, 60));
        // the slow render is inside its plugin's draw: the quick one must be able to finish now
        let waited = entering.recv_timeout(Duration::from_secs(10));
        let quick_render = waited.map_err(|error| error.to_string()).and_then(|()| {
            let done = rendered(&gpu, &quick, 60);
            go.send(()).map_err(|error| error.to_string())?;
            done.map_err(|error| error.to_string())
        });
        let _ = go.send(());
        (slow_render.join(), quick_render)
    });
    assert!(quick_result.is_ok(), "{quick_result:?}");
    assert!(matches!(slow_result, Ok(Ok(_))), "the slow render: {slow_result:?}");
    Ok(())
}
