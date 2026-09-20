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
    // eframe decodes it: `image` is already linked in through eframe itself, and
    // `from_png_bytes` converts whatever colour type the file has rather than
    // refusing anything that is not RGBA8. A failure costs the icon and nothing
    // else, which is the right trade for decoration.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-128.png")) {
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
