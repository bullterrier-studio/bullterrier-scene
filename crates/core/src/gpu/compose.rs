//! A scene composed on the device: `render::draw` and its helpers (`lay`, the mask planes, the feather) as recorded kernels.
//!
//! What the CPU decides is decided by the CPU's own code (`land_by`: where a layer lands, which map puts it there, what it is
//! drawn from); the device does the per-pixel work: the average of a shrunk raster, the warp, the mask planes and their
//! feather, the blend. A frame records everything into one command encoder and submits it once.

use std::sync::Arc;

use wgpu::Buffer;

use super::Gpu;
use super::executor::{Bindings, Call, Executor, Kernel};
use crate::cache::{Cache, Keyed};
use crate::feather;
use crate::geometry::Rect;
use crate::landing::{Landed, Reducing, Target, land_by};
use crate::layer::{Body, Join};
use crate::mask::{CanvasMask, Use};
use crate::render::{checked_size, target_of};
use crate::source::picture::{Look, Resident};
use crate::warp::{
    Map, Output, fixed_footprint, inverse_homography, is_exact_class, is_identity_of, reduced_map, reduction,
};
use crate::{Error, Layer, Scene};

/// A buffer on the device, as the cache keeps it.
struct Stored(Buffer);

/// A layer's looks run on the device, or none where they are looked on the CPU.
struct Looked(Option<Buffer>);

/// [`Gpu::render`].
pub(super) async fn render(gpu: &Gpu, scene: &Scene, size: i128) -> Result<(Buffer, usize, usize), Error> {
    let (width, height, layers, cpu_cache) = scene.parts();
    let (columns, rows) = checked_size(width, height, size)?;
    gpu.fits(columns, rows)?;
    let device_cache = gpu.cache();
    // phase one, on the CPU and under no lock: what each layer's source draws (a drawing, a text, a plugin kind, however long
    // it takes) and where it lands; phase two records and submits what phase one made
    cpu_cache.begin_render(false);
    let target = target_of(width, height, (columns, rows), (0, rows), cpu_cache);
    let mut landings = land_layers(layers, &target);
    let made = gpu
        .guarded("compose a scene", || {
            device_cache.begin_render(false);
            let mut frame = Frame {
                gpu,
                encoder: gpu.inner.executor.encoder(),
                cache: Arc::clone(&device_cache),
                scratch: Vec::new(),
            };
            let joined = frame.draw(layers, &target, &mut landings)?;
            let picture = frame.buffer(columns * rows * 16)?;
            frame.run(
                Kernel::Straight,
                &Call::new(columns, rows),
                &Bindings {
                    src: Some(&joined),
                    dst: &picture,
                    aux: None,
                },
            );
            gpu.inner.executor.queue.submit([frame.encoder.finish()]);
            Ok(picture)
        })
        .await;
    let picture = match made {
        Ok(picture) => picture,
        Err(error) => {
            // what the failed render kept was recorded and never submitted: a later render must not read it
            gpu.set_cache_budget(gpu.cache_budget());
            return Err(error);
        }
    };
    Ok((picture, rows, columns))
}

/// What `layer` draws its source from at `target`: the landing of the source without its looks where they run on the device (so
/// the raster that is sent is the unlooked one), else of the whole.
fn land_for(body: &Body, target: &Target, light: bool) -> Result<Landed, Error> {
    if on_device(body, light) {
        land_by(&body.without_looks(), target, light, Reducing::Later)
    } else {
        land_by(body, target, light, Reducing::Later)
    }
}

/// Whether `body`'s looks all run on the device (and it has some): then they are run there, on the raster the source drew.
fn on_device(body: &Body, light: bool) -> bool {
    let looks = body.finish.looks();
    !light && !looks.is_empty() && looks.iter().all(Look::on_device)
}

/// The landing of each layer's own source on `target`, in layer order, with the error of a layer that cannot land named for it;
/// none for a layer that draws nothing (opacity 0) or has no body, which `lay` answers as the CPU does. Layers of masks land
/// where the mask is recorded.
fn land_layers(layers: &[Layer], target: &Target) -> Vec<Option<Result<Landed, Error>>> {
    layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            if layer.join.opacity == 0.0 {
                return None;
            }
            let body = layer.drawable().ok()?;
            let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
            Some(land_for(body, target, layer.join.light).map_err(|error| error.in_layer(&name)))
        })
        .collect()
}

/// One render's recording.
struct Frame<'a> {
    gpu: &'a Gpu,
    encoder: wgpu::CommandEncoder,
    cache: Arc<Cache>,
    /// Buffers a layer lands in, reused by the next of the same size: each is read before the next layer's warp is recorded.
    scratch: Vec<Buffer>,
}

impl Frame<'_> {
    fn executor(&self) -> &Executor {
        &self.gpu.inner.executor
    }

    fn run(&mut self, kernel: Kernel, call: &Call, bindings: &Bindings<'_>) {
        self.gpu
            .inner
            .executor
            .dispatch(&mut self.encoder, kernel, call, bindings);
    }

    /// A zeroed buffer of `bytes` bytes.
    ///
    /// # Errors
    /// [`Error::Gpu`] for one larger than the device binds.
    fn buffer(&self, bytes: usize) -> Result<Buffer, Error> {
        let largest = self.executor().largest_binding();
        if bytes as u64 > largest {
            return Err(Error::Gpu {
                message: format!(
                    "a layer or plane of {} MiB is larger than the GPU binds ({} MiB on {}): render it on the CPU",
                    bytes >> 20,
                    largest >> 20,
                    self.gpu.inner.adapter
                ),
            });
        }
        Ok(self.executor().buffer(bytes as u64))
    }

    /// A buffer of `bytes` bytes that holds a layer for a moment: the same buffer again if the last of this size has been read.
    fn landing(&mut self, bytes: usize) -> Result<Buffer, Error> {
        if let Some(found) = self
            .scratch
            .iter()
            .find(|buffer| buffer.size() == (bytes as u64).max(16))
        {
            return Ok(found.clone());
        }
        let made = self.buffer(bytes)?;
        self.scratch.push(made.clone());
        Ok(made)
    }

    /// `layers` one over the other on `target`: premultiplied RGBA on the device.
    fn draw(
        &mut self,
        layers: &[Layer],
        target: &Target,
        landings: &mut [Option<Result<Landed, Error>>],
    ) -> Result<Buffer, Error> {
        let (columns, rows) = target.drawn_size();
        let joined = self.buffer(columns * rows * 16)?;
        for (index, layer) in layers.iter().enumerate() {
            let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
            self.lay(layer, target, &joined, landings[index].take())
                .map_err(|error| error.in_layer(&name))?;
        }
        Ok(joined)
    }

    /// `layer` laid over `joined` (`render::lay`): its source drawn and put where it belongs, each pixel blended at its opacity
    /// and its masks' product.
    fn lay(
        &mut self,
        layer: &Layer,
        target: &Target,
        joined: &Buffer,
        landed: Option<Result<Landed, Error>>,
    ) -> Result<(), Error> {
        if layer.join.opacity == 0.0 {
            return Ok(());
        }
        let factor = self.product(&layer.join, target)?;
        let Join {
            opacity, blend, light, ..
        } = layer.join;
        let body = layer.drawable()?;
        let pixels = self.layer(body, target, light, landed.transpose()?)?;
        let (columns, rows) = target.drawn_size();
        let flags = usize::from(light) | (usize::from(factor.is_some()) << 1);
        self.run(
            Kernel::Join,
            &Call::new(columns, rows)
                .int(2, blend as usize)
                .int(3, flags)
                .real(0, f64::from(opacity)),
            &Bindings {
                src: Some(&pixels),
                dst: joined,
                aux: factor.as_ref(),
            },
        );
        Ok(())
    }

    /// The raster `landed` is drawn from, on the device: kept under the CPU cache's key for it.
    fn raster(&self, landed: &Landed) -> Result<Buffer, Error> {
        let raster = &landed.drawn.raster;
        let upload = || {
            let bytes = size_of_val(raster.pixels());
            self.buffer(bytes)?;
            self.gpu.count_upload(bytes);
            Ok::<_, Error>((Stored(self.executor().sent(raster.pixels())), bytes))
        };
        // the key the CPU cache keeps it under, or, for a picture the CPU holds as it is (not kept there), the digest of its pixels
        // (made once for a picture, however many scenes hold it)
        let mut kept = Keyed::new("gpu-raster");
        match landed.key {
            Some(key) => kept.digest(&key),
            None => kept.digest(raster.digest()),
        };
        Ok(self.cache.fetch(&kept.finish(), upload)?.0.clone())
    }

    /// `body` drawn and put on `target`. Looks that image can run on the device are run there on the raster the source drew, which
    /// is sent once; any other layer is drawn as on the CPU and its raster sent.
    fn layer(&mut self, body: &Body, target: &Target, light: bool, landed: Option<Landed>) -> Result<Buffer, Error> {
        let landed = match landed {
            Some(landed) => landed,
            None => land_for(body, target, light)?,
        };
        if on_device(body, light) {
            // the looks leave the alpha as it is and the size, so the raster the source drew places as the looked one does
            if let Some(looked) = self.looked(&landed, body.finish.looks())? {
                return self.landed(&landed, Some(looked));
            }
            return self.landed(&land_by(body, target, light, Reducing::Later)?, None);
        }
        self.landed(&landed, None)
    }

    /// `landed`'s raster with `looks` run on it on the device, kept there; none where image runs a look only on the CPU's size
    /// and frame (a recipe that resamples), which is then looked on the CPU.
    fn looked(&mut self, landed: &Landed, looks: &[Look]) -> Result<Option<Buffer>, Error> {
        let mut key = Keyed::new("gpu-looked");
        match landed.key {
            Some(kept) => key.digest(&kept),
            None => key.digest(landed.drawn.raster.digest()),
        };
        for look in looks {
            key.debug(&landed.request.canvas_of(look.px()));
            look.keyed(&mut key)?;
        }
        let cache = Arc::clone(&self.cache);
        let stored = cache.fetch(&key.finish(), || {
            let image = &self.gpu.inner.image;
            let raster = &landed.drawn.raster;
            let mut resident = Resident::upload(image, raster)?;
            for look in looks {
                match look.render_after(image, &resident, landed.request.canvas_of(look.px()))? {
                    Some(made) => resident = made,
                    None => return Ok((Looked(None), 0)),
                }
            }
            let bytes = raster.width() * raster.height() * 16;
            let out = self.buffer(bytes)?;
            self.run(
                Kernel::Recolour,
                &Call::new(raster.width(), raster.height()),
                &Bindings {
                    src: Some(resident.buffer()),
                    dst: &out,
                    aux: Some(resident.buffer()),
                },
            );
            Ok((Looked(Some(out)), bytes))
        })?;
        Ok(stored.0.clone())
    }

    /// `landed` put on its output: the pixels the layer lands on the target, straight RGBA, zero where it does not reach.
    fn landed(&mut self, landed: &Landed, looked: Option<Buffer>) -> Result<Buffer, Error> {
        let mut source = match looked {
            Some(looked) => looked,
            None => self.raster(landed)?,
        };
        let (mut map, mut clip) = (landed.map, landed.clip);
        let mut size = (landed.drawn.raster.width(), landed.drawn.raster.height());
        if let Map::Affine(affine) = map
            && let Some(to) = reduction(size, affine)
        {
            let reduced = |frame: &mut Self| frame.reduce(&source, size, to);
            source = match landed.key {
                Some(key) => {
                    let mut kept = Keyed::new("gpu-reduced");
                    kept.digest(&key).debug(&to);
                    let cache = Arc::clone(&self.cache);
                    cache
                        .fetch(&kept.finish(), || {
                            let buffer = reduced(self)?;
                            Ok((Stored(buffer), to.0 * to.1 * 16))
                        })?
                        .0
                        .clone()
                }
                None => reduced(self)?,
            };
            let (affine, reduced_clip) = reduced_map(size, to, affine, clip);
            (map, clip, size) = (Map::Affine(affine), reduced_clip, to);
        }
        let output = landed.output;
        if let Map::Affine(affine) = map
            && is_identity_of(size, affine, clip, (output.columns, output.rows))
        {
            return Ok(source);
        }
        let out = self.landing(output.columns * output.count * 16)?;
        self.run(
            Kernel::Warp,
            &warp_call(map, clip, output, size),
            &Bindings {
                src: Some(&source),
                dst: &out,
                aux: None,
            },
        );
        Ok(out)
    }

    /// `source` of `from` pixels averaged by area down to `to` (the CPU's `reduce`).
    fn reduce(&mut self, source: &Buffer, from: (usize, usize), to: (usize, usize)) -> Result<Buffer, Error> {
        let along = self.buffer(to.0 * from.1 * 16)?;
        self.run(
            Kernel::ReduceAcross,
            &Call::new(to.0, from.1).int(2, from.0),
            &Bindings {
                src: Some(source),
                dst: &along,
                aux: None,
            },
        );
        let reduced = self.buffer(to.0 * to.1 * 16)?;
        self.run(
            Kernel::ReduceDown,
            &Call::new(to.0, to.1).int(2, from.1),
            &Bindings {
                src: Some(&along),
                dst: &reduced,
                aux: None,
            },
        );
        Ok(reduced)
    }

    /// The product of the planes of a layer's masks, or none for a layer with none (`mask::product`).
    fn product(&mut self, join: &Join, target: &Target) -> Result<Option<Buffer>, Error> {
        let (columns, rows) = target.drawn_size();
        let mut product: Option<Buffer> = None;
        for mask in &join.masks {
            let plane = self
                .plane(mask, target)
                .map_err(|error| error.in_layer(&crate::error::label(mask.number())))?;
            product = Some(match product {
                None => plane,
                Some(so_far) => self.planes(0, (columns, rows), &so_far, Some(&plane))?,
            });
        }
        Ok(product)
    }

    /// The product of two planes (`operation` 0) or one minus a plane (1).
    fn planes(
        &mut self,
        operation: usize,
        (columns, rows): (usize, usize),
        plane: &Buffer,
        other: Option<&Buffer>,
    ) -> Result<Buffer, Error> {
        let out = self.buffer(columns * rows * 4)?;
        self.run(
            Kernel::Planes,
            &Call::new(columns, rows).int(2, operation),
            &Bindings {
                src: Some(plane),
                dst: &out,
                aux: other,
            },
        );
        Ok(out)
    }

    /// One mask's plane on `target`, kept on the device (`CanvasMask::plane`).
    fn plane(&mut self, mask: &CanvasMask, target: &Target) -> Result<Buffer, Error> {
        let mut key = Keyed::new("gpu-plane");
        mask.layer().keyed(&mut key)?;
        key.debug(&(mask.uses, mask.feather, mask.invert));
        target.keyed(&mut key);
        let (columns, rows) = target.drawn_size();
        let cache = Arc::clone(&self.cache);
        let stored = cache.fetch(&key.finish(), || {
            let plane = self.plane_drawn(mask, target)?;
            Ok((Stored(plane), columns * rows * 4))
        })?;
        Ok(stored.0.clone())
    }

    /// `CanvasMask::drawn`: the layer landed on the target grown by the feather's reach, as values, blurred, cut back, inverted.
    fn plane_drawn(&mut self, mask: &CanvasMask, target: &Target) -> Result<Buffer, Error> {
        let sigma = (
            mask.feather * target.scale.x.pixels(1.0),
            mask.feather * target.scale.y.pixels(1.0),
        );
        let reach = (feather::reach(sigma.0), feather::reach(sigma.1));
        feather::fits("feather", target.drawn_size(), reach)?;
        let wide = target.grown(reach, feather::box_width(sigma.1));
        let layer = mask.layer();
        let factor = self.product(&layer.join, &wide)?;
        let body = layer.drawable()?;
        let unlooked;
        let body = if mask.uses == Use::Alpha && body.finish.is_looked() {
            unlooked = body.without_looks();
            &unlooked
        } else {
            body
        };
        let pixels = self.layer(body, &wide, false, None)?;
        let size = wide.drawn_size();
        let mut plane = self.buffer(size.0 * size.1 * 4)?;
        self.run(
            Kernel::Plane,
            &Call::new(size.0, size.1)
                .int(2, usize::from(mask.uses == Use::Luminance))
                .int(3, usize::from(factor.is_some()))
                .real(0, f64::from(layer.join.opacity)),
            &Bindings {
                src: Some(&pixels),
                dst: &plane,
                aux: factor.as_ref(),
            },
        );
        if reach != (0, 0) {
            plane = self.blur(plane, size, sigma)?;
            let from = (reach.0, target.window.0 + reach.1 - wide.window.0);
            plane = self.window(
                &plane,
                size,
                target.drawn_size(),
                (
                    i64::try_from(from.0).unwrap_or(i64::MAX),
                    i64::try_from(from.1).unwrap_or(i64::MAX),
                ),
            )?;
        }
        if mask.invert {
            plane = self.planes(1, target.drawn_size(), &plane, None)?;
        }
        Ok(plane)
    }

    /// A rectangle of `plane` (`from` columns by rows), `out` of it from the offset `at`, the edge's value repeated past it.
    fn window(
        &mut self,
        plane: &Buffer,
        from: (usize, usize),
        out: (usize, usize),
        at: (i64, i64),
    ) -> Result<Buffer, Error> {
        let made = self.buffer(out.0 * out.1 * 4)?;
        self.run(
            Kernel::Window,
            &Call::new(out.0, out.1)
                .int(2, from.0)
                .int(3, from.1)
                .signed(4, at.0)
                .signed(5, at.1),
            &Bindings {
                src: Some(plane),
                dst: &made,
                aux: None,
            },
        );
        Ok(made)
    }

    /// `feather::blur`: three box passes along x, then along y, each on the line padded by the pass's reach with its ends.
    fn blur(
        &mut self,
        mut plane: Buffer,
        size: (usize, usize),
        (sigma_x, sigma_y): (f64, f64),
    ) -> Result<Buffer, Error> {
        for (axis, sigma) in [(0_usize, sigma_x), (1, sigma_y)] {
            if sigma <= 0.0 {
                continue;
            }
            let (n, f) = feather::box_of(sigma);
            let reach = feather::reach(sigma);
            let wide = if axis == 0 {
                (size.0 + 2 * reach, size.1)
            } else {
                (size.0, size.1 + 2 * reach)
            };
            let pad = i64::try_from(reach).unwrap_or(i64::MAX);
            let (before_x, before_y) = if axis == 0 { (-pad, 0) } else { (0, -pad) };
            let mut line = self.window(&plane, size, wide, (before_x, before_y))?;
            for _ in 0..3 {
                let next = self.buffer(wide.0 * wide.1 * 4)?;
                self.run(
                    Kernel::Blur,
                    &Call::new(wide.0, wide.1).int(2, axis).int(3, n).real(0, f),
                    &Bindings {
                        src: Some(&line),
                        dst: &next,
                        aux: None,
                    },
                );
                line = next;
            }
            plane = self.window(&line, wide, size, (-before_x, -before_y))?;
        }
        Ok(plane)
    }
}

/// The call that runs the warp kernel: `map` from a raster of `source` pixels, clipped to `clip`, onto `output`.
fn warp_call(map: Map, clip: Rect, output: Output, source: (usize, usize)) -> Call {
    let mut call = Call::new(output.columns, output.count)
        .int(2, source.0)
        .int(3, source.1)
        .int(5, output.first)
        .real(8, clip.x)
        .real(9, clip.x + clip.width)
        .real(10, clip.y)
        .real(11, clip.y + clip.height);
    match map {
        Map::Affine(affine) => {
            let inverse = affine.inverse();
            for (at, value) in [inverse.a, inverse.b, inverse.c, inverse.d, inverse.e, inverse.f]
                .into_iter()
                .enumerate()
            {
                call = call.real(at, value);
            }
            call = call
                .real(6, inverse.a.hypot(inverse.b))
                .real(7, inverse.d.hypot(inverse.e));
            if is_exact_class(affine) {
                call = call.int(4, 0);
            } else {
                let fixed = fixed_footprint(affine, source);
                call = call
                    .int(4, 1)
                    .int(7, usize::from(fixed.plain))
                    .int(8, fixed.taps.0)
                    .int(9, fixed.taps.1)
                    .real(12, fixed.lengths.0)
                    .real(13, fixed.lengths.1)
                    .real(14, fixed.along.0)
                    .real(15, fixed.along.1);
            }
        }
        Map::Projective(projective) => {
            let inverse = inverse_homography(projective, clip);
            call = call.int(4, 2).real(
                28,
                f64::from(u32::try_from(source.0).unwrap_or(u32::MAX))
                    .hypot(f64::from(u32::try_from(source.1).unwrap_or(u32::MAX))),
            );
            for (row, values) in inverse.iter().enumerate() {
                for (column, value) in values.iter().enumerate() {
                    call = call.real(16 + 4 * row + column, *value);
                }
            }
        }
    }
    call
}
