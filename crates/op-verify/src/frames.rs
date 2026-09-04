//! Compare the images two interaction reports drew, perceptually.
//!
//! A reproducible run is not a bit-identical one: a frame boundary falls
//! where it falls, and a transition caught a hundredth of a second later
//! paints slightly different pixels. So two runs' artefacts are held to
//! what a reader would notice instead. Each pair is reduced to a common
//! small size, which is also what removes differences no eye resolves,
//! and compared in CIEDE2000: the average difference must stay under one
//! unit, roughly the smallest a person can see side by side, and almost
//! no pixel may exceed three.
//!
//! The colour maths is op-colour's, the same the palette is fitted and
//! tested with, whose CIEDE2000 is checked against Sharma's published
//! pairs.

use crate::Outcome;
use op_colour::{Lab, Linear, Srgb, ciede2000};
use png::{ColorType, Decoder, Limits, Transformations};
use std::path::{Path, PathBuf};

/// The longest side both images are reduced to before they are compared.
pub const MAX_SIDE: u32 = 256;
/// The limit on the average CIEDE2000 over an image.
pub const MEAN_LIMIT: f64 = 1.0;
/// No more than [`OUTLIER_SHARE`] of an image's pixels may pass this.
pub const OUTLIER_LIMIT: f64 = 3.0;
/// The share of pixels allowed past [`OUTLIER_LIMIT`].
pub const OUTLIER_SHARE: f64 = 0.01;

/// The report's filmstrips run one frame wide per step, so an image can be
/// tens of thousands of pixels across; the png crate's 64 MiB default
/// would refuse them.
const DECODE_BUDGET: usize = 1 << 30;

/// An image as 8-bit sRGB, three bytes a pixel, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// What a pair of images came to: the mean CIEDE2000 over them, the share
/// of pixels past [`OUTLIER_LIMIT`], and a note for the reader.
#[derive(Clone, Debug, PartialEq)]
pub struct Difference {
    pub mean: f64,
    pub share: f64,
    pub note: String,
}

impl Difference {
    /// Whether a reader would see the two apart.
    pub fn visible(&self) -> bool {
        self.mean > MEAN_LIMIT || self.share > OUTLIER_SHARE
    }
}

/// Decode a PNG to 8-bit sRGB, dropping any alpha and expanding any
/// palette, naming the file in anything it has to refuse.
pub fn decode(path: &Path) -> Result<Image, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| fail(&e))?;
    let mut decoder = Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(Transformations::normalize_to_color8());
    decoder.set_limits(Limits {
        bytes: DECODE_BUDGET,
    });
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| fail(&"too large to decode"))?;
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer).map_err(|e| fail(&e))?;
    buffer.truncate(info.buffer_size());
    let rgb = match info.color_type {
        ColorType::Rgb => buffer,
        ColorType::Rgba => buffer
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g]).collect(),
        ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0]; 3])
            .collect(),
        ColorType::Indexed => return Err(fail(&"a palette the decoder left unexpanded")),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgb,
    })
}

/// The common size a pair is reduced to: the same shape, longest side at
/// most [`MAX_SIDE`], never smaller than a pixel.
pub fn target_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (f64::from(MAX_SIDE) / f64::from(width.max(height))).min(1.0);
    let side = |n: u32| ((f64::from(n) * scale).round() as u32).max(1);
    (side(width), side(height))
}

/// Box-average an image down to `target_w` by `target_h` in linear light:
/// averaging gamma-encoded channels darkens every edge it crosses. The
/// target is clamped to the image, so no box is ever empty. An image
/// whose buffer is not three bytes a pixel reduces to nothing.
pub fn reduce(image: &Image, target_w: u32, target_h: u32) -> Vec<u8> {
    let (width, height) = (u64::from(image.width), u64::from(image.height));
    if image.rgb.len() as u64 != width * height * 3 {
        return Vec::new();
    }
    let across = u64::from(target_w.max(1).min(image.width));
    let down = u64::from(target_h.max(1).min(image.height));
    let mut out = Vec::with_capacity((across * down * 3) as usize);
    for y in 0..down {
        let (top, bottom) = (y * height / down, (y + 1) * height / down);
        for x in 0..across {
            let (left, right) = (x * width / across, (x + 1) * width / across);
            let mut sum = Linear {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            };
            for row in top..bottom {
                let from = ((row * width + left) * 3) as usize;
                let to = from + ((right - left) * 3) as usize;
                for pixel in image.rgb[from..to].as_chunks::<3>().0 {
                    let light = srgb(pixel).to_linear();
                    sum.r += light.r;
                    sum.g += light.g;
                    sum.b += light.b;
                }
            }
            let n = ((right - left) * (bottom - top)) as f64;
            let mean = Linear {
                r: sum.r / n,
                g: sum.g / n,
                b: sum.b / n,
            }
            .to_srgb();
            out.extend_from_slice(&[byte(mean.r), byte(mean.g), byte(mean.b)]);
        }
    }
    out
}

/// The mean CIEDE2000 between two equally sized 8-bit sRGB buffers, and
/// the share of pixels past [`OUTLIER_LIMIT`].
pub fn distance(a: &[u8], b: &[u8]) -> (f64, f64) {
    let mut total = 0.0;
    let mut outliers = 0usize;
    let mut counted = 0usize;
    for (p, q) in a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0) {
        counted += 1;
        if p == q {
            continue;
        }
        let d = ciede2000(Lab::from_srgb(srgb(p)), Lab::from_srgb(srgb(q)));
        total += d;
        if d > OUTLIER_LIMIT {
            outliers += 1;
        }
    }
    let n = counted as f64;
    (total / n, outliers as f64 / n)
}

/// Reduce a pair to a common size and measure it. Images of different
/// sizes, or with nothing to compare, are infinitely far apart.
pub fn compare_images(a: &Image, b: &Image) -> Difference {
    if (a.width, a.height) != (b.width, b.height) {
        return apart(format!(
            "different sizes, {}x{} and {}x{}",
            a.width, a.height, b.width, b.height
        ));
    }
    let (across, down) = target_size(a.width, a.height);
    let (pa, pb) = (reduce(a, across, down), reduce(b, across, down));
    if pa.is_empty() || pb.is_empty() {
        return apart("no pixels to compare".to_owned());
    }
    let (mean, share) = distance(&pa, &pb);
    Difference {
        mean,
        share,
        note: format!("{across}x{down} compared"),
    }
}

fn apart(note: String) -> Difference {
    Difference {
        mean: f64::INFINITY,
        share: 1.0,
        note,
    }
}

fn srgb(pixel: &[u8; 3]) -> Srgb {
    Srgb {
        r: f64::from(pixel[0]) / 255.0,
        g: f64::from(pixel[1]) / 255.0,
        b: f64::from(pixel[2]) / 255.0,
    }
}

fn byte(channel: f64) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Every PNG the second report drew that the first drew too, as the pair
/// of paths, in the order the second's tree sorts.
pub fn pairs(first: &Path, again: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut drawn = Vec::new();
    walk(again, &mut drawn);
    drawn.sort();
    drawn
        .into_iter()
        .filter_map(|drew| {
            let other = first.join(drew.strip_prefix(again).ok()?);
            other.is_file().then_some((other, drew))
        })
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "png") {
            out.push(path);
        }
    }
}

/// Hold every image of the second report against the first's.
pub fn compare(first: &Path, again: &Path) -> Outcome {
    let found = pairs(first, again);
    if found.is_empty() {
        return Outcome {
            differences: Vec::new(),
            summary: format!(
                "no image in {} has a counterpart in {}",
                again.display(),
                first.display()
            ),
            failed: true,
        };
    }
    let mut differences = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    let mut worst = 0.0f64;
    for (a, b) in &found {
        let name = b.strip_prefix(again).unwrap_or(b).display().to_string();
        let pair = decode(a).and_then(|first| decode(b).map(|again| (first, again)));
        let (ia, ib) = match pair {
            Ok(images) => images,
            // an image that will not decode is one that did not reproduce;
            // the refusal already names the file it came from
            Err(why) => {
                differences.push(why);
                bad.push(name);
                continue;
            }
        };
        let seen = compare_images(&ia, &ib);
        worst = worst.max(seen.mean);
        if seen.visible() {
            differences.push(format!(
                "{name}: mean dE {:.2}, {:.2}% of pixels past {OUTLIER_LIMIT:.1} ({})",
                seen.mean,
                seen.share * 100.0,
                seen.note
            ));
            bad.push(name);
        }
    }
    if bad.is_empty() {
        return Outcome {
            differences,
            summary: format!(
                "{} images redrawn indistinguishably, worst mean dE {worst:.2} against a limit of {MEAN_LIMIT:.1}",
                found.len()
            ),
            failed: false,
        };
    }
    Outcome {
        summary: format!("images a reader would see differ: {}", bad.join(", ")),
        differences,
        failed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat field of one colour.
    fn flat(width: u32, height: u32, colour: [u8; 3]) -> Image {
        Image {
            width,
            height,
            rgb: colour.repeat((width * height) as usize),
        }
    }

    /// The same image with rows `band` repainted.
    fn banded(base: &Image, band: std::ops::Range<u32>, colour: [u8; 3]) -> Image {
        let mut out = base.clone();
        for row in band {
            let from = (row * base.width * 3) as usize;
            let to = from + (base.width * 3) as usize;
            out.rgb[from..to].copy_from_slice(&colour.repeat(base.width as usize));
        }
        out
    }

    const GREY: [u8; 3] = [0x80, 0x80, 0x80];
    const ORANGE: [u8; 3] = [0xC0, 0x80, 0x40];

    #[test]
    fn an_image_against_itself_passes() {
        let one = flat(64, 64, GREY);
        let seen = compare_images(&one, &one);
        assert_eq!(seen.mean, 0.0);
        assert_eq!(seen.share, 0.0);
        assert!(!seen.visible());
        assert_eq!(seen.note, "64x64 compared");
    }

    #[test]
    fn a_few_single_level_pixels_pass() {
        let one = flat(64, 64, GREY);
        let mut other = one.clone();
        for i in 0..30 {
            other.rgb[i * 3] += 1;
        }
        let seen = compare_images(&one, &other);
        assert!(seen.mean > 0.0, "the difference must be measured, {seen:?}");
        assert!(!seen.visible(), "{seen:?}");
    }

    #[test]
    fn a_repainted_band_fails() {
        let one = flat(64, 64, GREY);
        let seen = compare_images(&one, &banded(&one, 28..36, ORANGE));
        assert!(seen.visible(), "{seen:?}");
        assert!(seen.mean > MEAN_LIMIT, "{seen:?}");
    }

    /// The outlier rule on its own: two rows in a hundred are far apart,
    /// which the mean alone would forgive.
    #[test]
    fn a_thin_repainted_line_fails_on_outliers_alone() {
        let one = flat(100, 100, GREY);
        let seen = compare_images(&one, &banded(&one, 40..42, ORANGE));
        assert!(seen.mean <= MEAN_LIMIT, "{seen:?}");
        assert!(seen.share > OUTLIER_SHARE, "{seen:?}");
        assert!(seen.visible(), "{seen:?}");
    }

    #[test]
    fn a_size_mismatch_fails() {
        let seen = compare_images(&flat(64, 64, GREY), &flat(64, 32, GREY));
        assert!(seen.visible());
        assert_eq!(seen.note, "different sizes, 64x64 and 64x32");
    }

    #[test]
    fn an_image_with_no_pixels_fails() {
        let seen = compare_images(&flat(0, 0, GREY), &flat(0, 0, GREY));
        assert!(seen.visible());
        assert_eq!(seen.note, "no pixels to compare");
    }

    #[test]
    fn a_long_image_is_reduced_to_the_longest_side() {
        assert_eq!(target_size(48096, 586), (256, 3));
        assert_eq!(target_size(200, 100), (200, 100));
        assert_eq!(target_size(512, 256), (256, 128));
        // averaging happens in linear light: half black and half white is
        // the mid grey a reader sees, not the 128 a naive average gives
        let mut pair = flat(2, 1, [0, 0, 0]);
        pair.rgb[3..].copy_from_slice(&[255, 255, 255]);
        assert_eq!(reduce(&pair, 1, 1), [188, 188, 188]);
        // the target is clamped to the image, so asking for more is asking
        // for the image itself
        assert_eq!(reduce(&pair, 9, 9), pair.rgb);
        // and a buffer that is not three bytes a pixel reduces to nothing
        let ragged = Image {
            width: 2,
            height: 1,
            rgb: vec![0, 0, 0],
        };
        assert!(reduce(&ragged, 1, 1).is_empty());
    }

    /// The decode path, once, over a real file: what a PNG holds is what
    /// comes back, and a pair on disk compares as the buffers do.
    #[test]
    fn a_png_is_decoded_and_compared_on_disk() {
        let dir = std::env::temp_dir().join(format!("op-verify-{}", std::process::id()));
        let tree = dir.join("again").join("adhoc");
        std::fs::create_dir_all(&tree).expect("a place to write");
        std::fs::create_dir_all(dir.join("first").join("adhoc")).expect("a place to write");
        let one = flat(64, 64, GREY);
        let other = banded(&one, 28..36, ORANGE);
        write_png(&dir.join("first/adhoc/film.png"), &one);
        write_png(&tree.join("film.png"), &other);

        let read = decode(&dir.join("first/adhoc/film.png")).expect("the png reads");
        assert_eq!(read, one);
        assert_eq!(
            pairs(&dir.join("first"), &dir.join("again")),
            [(
                dir.join("first/adhoc/film.png"),
                dir.join("again/adhoc/film.png")
            )]
        );
        let outcome = compare(&dir.join("first"), &dir.join("again"));
        assert!(outcome.failed, "{outcome:?}");
        assert!(
            outcome.differences[0].starts_with("adhoc/film.png: mean dE "),
            "{outcome:?}"
        );

        // the same image on both sides is the passing case, end to end
        write_png(&tree.join("film.png"), &one);
        let outcome = compare(&dir.join("first"), &dir.join("again"));
        assert!(!outcome.failed, "{outcome:?}");
        assert_eq!(
            outcome.summary,
            "1 images redrawn indistinguishably, worst mean dE 0.00 against a limit of 1.0"
        );

        // an image that will not decode did not reproduce either
        let half = std::fs::read(tree.join("film.png")).expect("the png reads");
        std::fs::write(tree.join("film.png"), &half[..half.len() / 2]).expect("a short png");
        let outcome = compare(&dir.join("first"), &dir.join("again"));
        assert!(outcome.failed, "{outcome:?}");
        assert!(
            outcome.differences[0].ends_with("unexpected end of file"),
            "{outcome:?}"
        );

        // and an empty tree has nothing to say
        let outcome = compare(&dir.join("first"), &dir.join("nowhere"));
        assert!(outcome.failed);
        assert!(outcome.summary.starts_with("no image in "), "{outcome:?}");
        std::fs::remove_dir_all(&dir).expect("the temporary tree goes");
    }

    fn write_png(path: &Path, image: &Image) {
        let file = std::fs::File::create(path).expect("a file to write");
        let mut encoder =
            png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("a header")
            .write_image_data(&image.rgb)
            .expect("the pixels");
    }
}
