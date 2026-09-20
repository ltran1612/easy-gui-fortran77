//! Easy Fortran 77.
//!
//! One window, one green button. The design constraint that decides every
//! argument: someone with no command-line experience must be able to open this, click one button, and see the user's
//! program run — and must never be able to damage the user's source files.

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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 800.0])
            .with_min_inner_size([760.0, 560.0])
            .with_title("Easy Fortran 77"),
        ..Default::default()
    };

    eframe::run_native(
        "Easy Fortran 77",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
