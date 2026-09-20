//! Easy Fortran 77.
//!
//! One window, one green button. The design constraint that decides every
//! argument: someone with no command-line experience must be able to open this,
//! click one button and see their program run — and must never be able to damage
//! their source files.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod markdown;
mod theme;

/// The window icon, decoded from what `cargo xtask gen-icons` produced.
///
/// Windows takes an executable's icon from its resources, which `build.rs`
/// embeds; this is what gives every other platform one. Returning `None` costs
/// the icon and nothing else, which is the right trade for decoration.
fn window_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../assets/icon-128.png");

    let mut reader = png::Decoder::new(std::io::Cursor::new(PNG))
        .read_info()
        .ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    // The generator writes RGBA8. Anything else means the asset was replaced by
    // hand with something this cannot use.
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    rgba.truncate(info.buffer_size());
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("EF77_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 800.0])
        .with_min_inner_size([760.0, 560.0])
        .with_title("Easy Fortran 77");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Easy Fortran 77",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
