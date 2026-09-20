//! `cargo xtask gen-icons` — derive the build's icons from `logo.png`.
//!
//! One piece of artwork lives in the repository. Everything the build needs is
//! generated from it, so the window icon, the executable's icon and the
//! installer's icon cannot drift apart or disagree with the README.
//!
//! Two files come out:
//!
//! - `packaging/windows/icon.ico` — embedded into `easy-fortran-77.exe` as a
//!   resource, and used by NSIS for the installer and uninstaller. The installer
//!   icon is the one he actually sees first, on the file he downloads.
//! - `crates/ef-gui/assets/icon-128.png` — the window icon, set at runtime, which
//!   is how Linux gets one at all.

use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use image::ImageEncoder;
use std::fs;

/// Sizes Windows actually asks for. 256 is the largest an `.ico` can hold, and
/// the small ones are worth generating rather than letting the shell downscale:
/// a 16-pixel icon resampled from 512 comes out mush.
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// The window icon. 128 is comfortably more than any desktop draws in a title
/// bar or a task switcher.
const WINDOW_ICON: u32 = 128;

pub fn run(args: &[String]) -> Result<()> {
    if let Some(a) = args.first() {
        bail!("unknown option `{a}`\n\nUSAGE: cargo xtask gen-icons");
    }
    let root = crate::repo_root();
    let source = root.join("logo.png");
    let logo = image::open(&source)
        .with_context(|| format!("reading {}", source.display()))?
        .into_rgba8();
    println!(
        "source   {} ({}x{})",
        source.display(),
        logo.width(),
        logo.height()
    );

    let ico_path = root.join("packaging").join("windows").join("icon.ico");
    let png_path = root
        .join("crates")
        .join("ef-gui")
        .join("assets")
        .join(format!("icon-{WINDOW_ICON}.png"));

    let mut frames = Vec::new();
    for &size in ICO_SIZES {
        let scaled = image::imageops::resize(&logo, size, size, FilterType::Lanczos3);
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(&scaled, size, size, image::ExtendedColorType::Rgba8)
        .context("encoding an icon frame")?;
        frames.push((size, png));
    }

    fs::create_dir_all(ico_path.parent().unwrap())?;
    fs::write(&ico_path, ico_container(&frames)?)?;
    println!(
        "ico      {} ({} sizes, {:.0} KB)",
        ico_path.display(),
        frames.len(),
        fs::metadata(&ico_path)?.len() as f64 / 1e3
    );

    let window = image::imageops::resize(&logo, WINDOW_ICON, WINDOW_ICON, FilterType::Lanczos3);
    fs::create_dir_all(png_path.parent().unwrap())?;
    window.save(&png_path)?;
    println!(
        "png      {} ({:.0} KB)",
        png_path.display(),
        fs::metadata(&png_path)?.len() as f64 / 1e3
    );
    Ok(())
}

/// Wrap PNG frames in an `.ico` container.
///
/// Hand-written rather than pulled from a crate because the format is a header
/// and a directory and nothing else. Every frame is stored as PNG, which Windows
/// has read inside an `.ico` since Vista and which keeps the file small.
fn ico_container(frames: &[(u32, Vec<u8>)]) -> Result<Vec<u8>> {
    const DIR_ENTRY: usize = 16;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());

    let mut offset = 6 + DIR_ENTRY * frames.len();
    for (size, png) in frames {
        if *size > 256 {
            bail!("{size} is larger than an .ico entry can describe");
        }
        // 256 is written as 0: the field is one byte and 256 does not fit.
        let dim = if *size == 256 { 0u8 } else { *size as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette size, 0 for truecolour
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in frames {
        out.extend_from_slice(png);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_frame(size: u32) -> (u32, Vec<u8>) {
        let img = image::RgbaImage::from_pixel(size, size, image::Rgba([1, 2, 3, 255]));
        let mut v = Vec::new();
        image::codecs::png::PngEncoder::new(&mut v)
            .write_image(&img, size, size, image::ExtendedColorType::Rgba8)
            .unwrap();
        (size, v)
    }

    #[test]
    fn the_container_describes_every_frame_and_points_at_it() {
        let frames: Vec<_> = [16u32, 256].iter().map(|s| png_frame(*s)).collect();
        let ico = ico_container(&frames).unwrap();

        assert_eq!(&ico[0..2], &[0, 0], "reserved");
        assert_eq!(u16::from_le_bytes([ico[2], ico[3]]), 1, "type is icon");
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 2, "two frames");

        for (i, (size, png)) in frames.iter().enumerate() {
            let e = 6 + 16 * i;
            // 256 has to be written as 0; the field is a single byte.
            let want = if *size == 256 { 0 } else { *size as u8 };
            assert_eq!(ico[e], want, "width of frame {i}");
            assert_eq!(ico[e + 1], want, "height of frame {i}");
            let len = u32::from_le_bytes(ico[e + 8..e + 12].try_into().unwrap()) as usize;
            let off = u32::from_le_bytes(ico[e + 12..e + 16].try_into().unwrap()) as usize;
            assert_eq!(len, png.len());
            // The offset must land exactly on that frame's PNG signature.
            assert_eq!(&ico[off..off + 8], &png[..8], "frame {i} is where it says");
            assert_eq!(&ico[off..off + len], &png[..], "frame {i} round-trips");
        }
        assert_eq!(
            ico.len(),
            6 + 32 + frames.iter().map(|(_, p)| p.len()).sum::<usize>()
        );
    }

    #[test]
    fn a_frame_too_large_to_describe_is_refused() {
        assert!(ico_container(&[png_frame(257)]).is_err());
    }
}
