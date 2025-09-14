//! Minimal CLI entrypoint performing an HTTP-based sync using Azure DevOps APIs.
use anyhow::Result;
use clap::Parser;
use modsync::{config::Config, downloader, http::AzureClient, index};

#[derive(Parser)]
struct Opts {
    /// Azure DevOps repository API base, e.g.
    /// https://dev.azure.com/{org}/{project}/_apis/git/repositories/{repo}
    #[arg(long)]
    base: String,

    /// Commit SHA to sync against
    #[arg(long)]
    commit: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();

    // Load configuration (for target directory)
    let cfg = Config::load()?;
    let out_dir = cfg.target_mod_dir.clone();
    // Build local index
    let local_index = index::build_local_index(&out_dir)?;

    // Create Azure client
    let client = AzureClient::new(&opts.base, None).await?;

    // List items at commit
    let items_resp = client.list_items_commit("/", &opts.commit).await?;

    // Build remote index from items
    let mut remote_index = index::Index::new();
    let pointer_prefix = b"version https://git-lfs.github.com/spec/";
    for item in items_resp.value.into_iter() {
        if item.is_folder.unwrap_or(false) {
            continue;
        }
        let path = match item.path {
            Some(p) => {
                // strip leading slash if present
                let p = if let Some(stripped) = p.strip_prefix('/') {
                    stripped
                } else {
                    p.as_str()
                };
                std::path::PathBuf::from(p)
            }
            None => continue,
        };
        if let Some(object_id) = item.object_id {
            // If the blob looks small, fetch content to detect LFS pointer
            let size = item.size.unwrap_or(0);
            if size > 0 && size < 4096 {
                if let Ok(bytes) = client.get_blob_by_oid(&object_id).await {
                    if bytes.starts_with(pointer_prefix) {
                        // parse pointer lines for oid sha256 and size
                        let s = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                        let mut oid_sha: Option<String> = None;
                        let mut sz: Option<u64> = None;
                        for line in s.lines() {
                            if let Some(stripped) = line.strip_prefix("oid sha256:") {
                                oid_sha = Some(stripped.trim().to_string());
                            } else if let Some(stripped) = line.strip_prefix("size ") {
                                if let Ok(v) = stripped.trim().parse::<u64>() {
                                    sz = Some(v);
                                }
                            }
                        }
                        if let Some(sha256) = oid_sha {
                            remote_index.insert(
                                path,
                                index::BlobEntry {
                                    oid: sha256,
                                    size: sz.unwrap_or(size),
                                    is_lfs: true,
                                },
                            );
                            continue;
                        }
                    } else {
                        // not a pointer; treat as regular blob with provided object id
                        remote_index.insert(
                            path,
                            index::BlobEntry {
                                oid: object_id,
                                size,
                                is_lfs: false,
                            },
                        );
                        continue;
                    }
                }
            } else {
                // Large blob -> regular file
                remote_index.insert(
                    path,
                    index::BlobEntry {
                        oid: object_id,
                        size,
                        is_lfs: false,
                    },
                );
                continue;
            }
        }
    }

    // Compute sync plan
    let plan = index::compare_indexes(&local_index, &remote_index);

    // Execute plan
    let concurrency = cfg.download_concurrency.unwrap_or(4);
    let summary = downloader::execute_plan(&client, plan, &out_dir, concurrency).await?;
    println!(
        "Sync complete: files_done={} bytes_done={}",
        summary.files_done, summary.bytes_done
    );
    Ok(())
}
