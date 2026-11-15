//! Own binary: a counting `#[global_allocator]` reads the whole process's peak heap, so no other test may share it.
//!
//! A PDF is written holding a few rows and a codec's buffers, never the compressed image: the most memory `write_pdf` holds
//! at once does not grow with the size of the file it writes.
//!
//! This is the only test of the binary, so nothing else allocates while it measures.

use bullterrier_scene::{Error, PdfOptions, Raster};
use peak_alloc::PeakAlloc;

#[global_allocator]
static HEAP: PeakAlloc = PeakAlloc;

const WIDTH: usize = 2000;
const HEIGHT: usize = 1200;

/// A picture of noise: Flate cannot shrink it, so each stream of the PDF is as large as the 8-bit plane it holds (the colour
/// 7.2 million bytes, the alpha 2.4 million). With `translucent`, every pixel has an alpha of its own.
fn noise(translucent: bool) -> Result<Raster, Error> {
    // xorshift64 (Marsaglia 2003): the same bytes on every run
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state.to_le_bytes()[0]
    };
    let mut pixels = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for _ in 0..WIDTH * HEIGHT {
        pixels.extend([next(), next(), next()].map(|byte| f32::from(byte) / 255.0));
        pixels.push(if translucent { f32::from(next()) / 255.0 } else { 1.0 });
    }
    Raster::rgba(WIDTH, HEIGHT, pixels)
}

#[test]
fn a_pdf_is_written_holding_far_less_than_its_size() -> Result<(), Box<dyn std::error::Error>> {
    let folder = tempfile::tempdir()?;
    let cases = [
        ("opaque: one stream", noise(false)?, PdfOptions::new(300.0)?),
        ("translucent: colour and alpha", noise(true)?, PdfOptions::new(300.0)?),
        (
            "bleed and crop marks",
            noise(true)?,
            PdfOptions::new(300.0)?.bleed_mm(3.0)?.crop_marks(true),
        ),
    ];
    for (what, raster, options) in cases {
        let path = folder.path().join("out.pdf");
        let before = HEAP.current_usage();
        HEAP.reset_peak_usage();
        raster.write_pdf(&path, &options)?;
        let held = HEAP.peak_usage() - before;
        // the file's own size, read back: the compressed streams are most of it
        let size = usize::try_from(std::fs::metadata(&path)?.len())?;
        assert!(size > 7_000_000, "{what}: the noise did not stay noise ({size} bytes)");
        assert!(
            held < size / 4,
            "{what}: {held} bytes held while writing a file of {size}"
        );
    }
    Ok(())
}
