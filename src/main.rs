#![windows_subsystem = "windows"]

mod app;
mod diff_parser;
mod git;
mod models;
mod theme;

use app::RustyGitApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("RustyGit - Fast Git GUI Client")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "RustyGit",
        native_options,
        Box::new(|cc| {
            // Hallmark theme: Warm Graphite, amber accent (by request).
            // All visuals come from tokens (src/theme.rs).
            theme::apply(&cc.egui_ctx);

            Ok(Box::new(RustyGitApp::new(cc)))
        }),
    )
}
