mod app;
mod audio;
mod theme;

use app::SchismaApp;
use eframe::egui;
use schisma_engine::rt_audit::AuditAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: AuditAllocator = AuditAllocator;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Schisma — Per-note physical instrument")
            .with_inner_size([1680.0, 1040.0])
            .with_min_inner_size([1024.0, 700.0])
            .with_maximized(false),
        renderer: eframe::Renderer::Wgpu,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "schisma",
        options,
        Box::new(|context| Ok(Box::new(SchismaApp::new(context)))),
    )
}
