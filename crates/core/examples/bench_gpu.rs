//! `uv run poe bench-gpu`: a 13-layer poster on the CPU and on the GPU, at 1,200 px and at 1080p: drawn cold, drawn again, and
//! with the top layer's and the bottom layer's opacity moved, as a slider does. The GPU is timed on the device alone (the work
//! waited for, nothing read back) and with the picture read back. Release build, one process; medians of several runs.

use std::io::Write;
use std::time::{Duration, Instant};

use bullterrier_scene::{Blend, Deg, Error, Fit, Gpu, Layer, Mask, Place, Raster, Scene, Values};
use pollster::block_on;

/// A picture of seeded noise.
fn noise(width: usize, height: usize, seed: u64) -> Result<Raster, Error> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let pixels = (0..width * height)
        .flat_map(|_| {
            [0; 4].map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                #[expect(clippy::cast_precision_loss, reason = "the top 24 bits of a random word")]
                {
                    (state >> 40) as f32 / (1_u64 << 24) as f32
                }
            })
        })
        .collect();
    Raster::rgba(width, height, pixels)
}

/// A poster of 1,200 by 800 design pixels: pictures (one shrunk past 2x, one turned, one in perspective, one looked, one masked by a
/// feathered picture), fills, a light layer, a group and blends.
fn poster() -> Result<Scene, Error> {
    let area = [0.0, 0.0, 1200.0, 800.0];
    let photo = |seed: u64, size: (usize, usize)| -> Result<Layer, Error> {
        Layer::picture(noise(size.0, size.1, seed)?).fit(area, Fit::Cover, [0.5, 0.5])
    };
    let grade = r#"{"steps": [{"effect": "grade", "params": {"contrast": 1.3, "saturation": 1.1}}]}"#;
    let inner = Scene::new(1200, 800)?.add([photo(9, (600, 400))?, Layer::fill("#ff8040")?.opacity(0.3)?])?;
    Scene::new(1200, 800)?.add([
        photo(1, (1200, 800))?.look(grade)?.named("l1")?,
        photo(2, (3000, 2000))?
            .place(Place::new().rotate(Deg(17.0)).scale(0.9))?
            .opacity(0.8)?
            .named("l2")?,
        Layer::fill("#305080")?
            .blend(Blend::Multiply)
            .opacity(0.6)?
            .named("l3")?,
        photo(3, (1200, 800))?
            .mask(Mask::new(photo(4, (300, 200))?).feather(12.0))?
            .named("l4")?,
        photo(5, (1200, 800))?
            .perspective([[100.0, 80.0], [1100.0, 140.0], [1050.0, 700.0], [150.0, 640.0]])?
            .opacity(0.9)?
            .named("l5")?,
        Layer::fill("#40a0ff")?
            .as_light(true)
            .blend(Blend::Screen)
            .opacity(0.5)?
            .named("l6")?,
        Layer::scene(inner).opacity(0.7)?.named("l7")?,
        photo(6, (1200, 800))?.blend(Blend::Overlay).opacity(0.5)?.named("l8")?,
        Layer::fill("#ffffff")?
            .blend(Blend::SoftLight)
            .opacity(0.3)?
            .named("l9")?,
        photo(7, (1200, 800))?.blend(Blend::Hue).opacity(0.4)?.named("l10")?,
        photo(8, (1200, 800))?
            .blend(Blend::Difference)
            .opacity(0.2)?
            .named("l11")?,
        Layer::fill("#201030")?.opacity(0.25)?.named("l12")?,
        photo(10, (1200, 800))?.opacity(0.5)?.named("l13")?,
    ])
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn ms(time: Duration) -> String {
    format!("{:8.1}", time.as_secs_f64() * 1000.0)
}

/// How a case makes its scene for run `n`.
type Make = Box<dyn FnMut(usize) -> Result<Scene, Error>>;

/// Runs of each case: the median of this many.
const RUNS: usize = 7;

/// The scene `make` gives for run `n`, rendered at `size` on the CPU: the median time.
fn cpu(size: u32, make: &mut impl FnMut(usize) -> Result<Scene, Error>) -> Result<Duration, Error> {
    let scenes: Vec<Scene> = (0..RUNS).map(&mut *make).collect::<Result<_, _>>()?;
    let mut times = Vec::new();
    for scene in &scenes {
        let start = Instant::now();
        scene.render(size)?;
        times.push(start.elapsed());
    }
    Ok(median(times))
}

/// The same on the GPU: with the work waited for on the device (nothing read back), and with the picture read back.
fn on_gpu(
    gpu: &Gpu,
    device: &wgpu::Device,
    size: u32,
    cold: bool,
    make: &mut impl FnMut(usize) -> Result<Scene, Error>,
) -> Result<(Duration, Duration), Box<dyn std::error::Error>> {
    let mut both = [Vec::new(), Vec::new()];
    for (read, times) in both.iter_mut().enumerate() {
        let scenes: Vec<Scene> = (0..RUNS).map(&mut *make).collect::<Result<_, _>>()?;
        for scene in &scenes {
            if cold {
                // what the device kept of the last run would make this one warm
                gpu.set_cache_budget(gpu.cache_budget());
            }
            let start = Instant::now();
            block_on(async {
                let held = gpu.render(scene, size).await?;
                if read == 1 {
                    gpu.download(&held).await?;
                } else {
                    device.poll(wgpu::PollType::wait_indefinitely())?;
                }
                Ok::<_, Box<dyn std::error::Error>>(())
            })?;
            times.push(start.elapsed());
        }
    }
    let [device_only, with_read] = both;
    Ok((median(device_only), median(with_read)))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (device, queue) = block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let wanted = adapter.limits();
        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: wanted.max_storage_buffer_binding_size,
            max_buffer_size: wanted.max_buffer_size,
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
    let gpu = Gpu::new(device.clone(), queue)?;
    let mut out = std::io::stdout();
    writeln!(out, "adapter: {}", gpu.adapter())?;
    writeln!(
        out,
        "{:34} {:>8} {:>8} {:>8}",
        "(ms, median of 7)", "CPU", "GPU", "GPU+read"
    )?;
    for size in [1200_u32, 1920] {
        writeln!(out, "-- {size} px on the long side")?;
        // a slider move: opacity of one layer, a new value each run
        let moved = |layer: &str, base: &Scene| {
            let (layer, base) = (layer.to_owned(), base.clone());
            move |run: usize| {
                base.with_values(&Values::new().set(
                    format!("{layer}.opacity"),
                    0.2 + 0.05 * f64::from(u8::try_from(run).unwrap_or(0)),
                ))
            }
        };
        let warm = poster()?;
        warm.render(size)?;
        block_on(gpu.render(&warm, size))?;
        let cases: Vec<(&str, Make, bool)> = vec![
            ("drawn cold (a scene made afresh)", Box::new(|_| poster()), true),
            (
                "drawn again",
                Box::new({
                    let warm = warm.clone();
                    move |_| Ok(warm.clone())
                }),
                false,
            ),
            ("top layer's opacity moved", Box::new(moved("l13", &warm)), false),
            ("bottom layer's opacity moved", Box::new(moved("l1", &warm)), false),
        ];
        for (name, mut make, cold) in cases {
            let cpu_time = cpu(size, &mut make)?;
            let (device_only, with_read) = on_gpu(&gpu, &device, size, cold, &mut make)?;
            writeln!(out, "{name:34} {} {} {}", ms(cpu_time), ms(device_only), ms(with_read))?;
        }
    }
    Ok(())
}
