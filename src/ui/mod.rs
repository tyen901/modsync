pub mod header;
pub mod settings_panel;
pub mod app;
pub mod file_graph;
pub use app::ModApp;

/// Run the native UI by building the app
pub fn run_ui() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ModSync",
        native_options,
        Box::new(|_cc| Ok(Box::new(ModApp::default()) as Box<dyn eframe::App>)),
    )
    .expect("Failed to start UI");
}
