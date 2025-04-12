mod config;
mod ui;

use librqbit::{
    Api,
    Session,
};
use tokio::sync::mpsc;
use anyhow::Context;
use config::load_config;
use ui::{MyApp, UiMessage};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration at startup
    let initial_config = load_config().context("Failed to load initial configuration")?;

    let options = eframe::NativeOptions::default();

    // Use configured download path (create if needed)
    let download_path = initial_config.local_download_path.clone();
    tokio::fs::create_dir_all(&download_path)
        .await
        .with_context(|| format!("Failed to create download directory: {:?}", download_path))?;

    let session = Session::new(download_path)
        .await
        .context("Failed to initialize librqbit session")?;

    let api = Api::new(session.clone(), None);

    eframe::run_native(
        "ModSync",
        options,
        Box::new(move |_cc| {
            let (tx, rx) = mpsc::unbounded_channel::<UiMessage>();
            // Pass initial config to MyApp
            // Clone initial_config as it's moved into the closure
            Ok(Box::new(MyApp::new(api.clone(), tx, rx, initial_config.clone())) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
