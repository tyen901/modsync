use anyhow::Result;
use std::fs;

use modsync::{gitutils, modpack};

fn main() -> Result<()> {
    // Replace this URL if you want to test a different repository.
    let url = "https://peanutcommunityarma@dev.azure.com/peanutcommunityarma/pca/_git/xyi";

    let base = std::env::temp_dir().join("modsync_try_clone");
    if base.exists() {
        let _ = fs::remove_dir_all(&base);
    }
    fs::create_dir_all(&base)?;

    let clone_path = base.join("cloned_repo");
    println!("Cloning {} -> {}", url, clone_path.display());

    let repo = gitutils::clone_or_open_repo(url, &clone_path)
        .map_err(|e| anyhow::anyhow!("clone_or_open_repo failed: {}", e))?;
    println!("Opened repo at {}", clone_path.display());

    println!("Fetching updates...");
    gitutils::fetch(&repo).map_err(|e| anyhow::anyhow!("fetch failed: {}", e))?;

    let target = base.join("target_mods");
    fs::create_dir_all(&target)?;

    println!("Running sync_modpack into {}", target.display());
    match modpack::sync_modpack(&clone_path, &target) {
        Ok(()) => println!("sync_modpack completed successfully"),
        Err(e) => println!("sync_modpack failed: {}", e),
    }

    println!("Listing target files under {}:", target.display());
    for entry in walkdir::WalkDir::new(&target)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        println!(" - {}", entry.path().display());
    }

    Ok(())
}
