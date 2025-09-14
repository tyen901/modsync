use anyhow::{Context, Error, Result};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Mapping of relative path -> blob entry
pub type Index = HashMap<PathBuf, BlobEntry>;

/// Entry describing a file found in an index (local or remote)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobEntry {
    pub oid: String,
    pub size: u64,
    pub is_lfs: bool,
}

/// Simple sync plan describing which paths need to be fetched.
#[derive(Clone, Debug, Default)]
pub struct SyncPlan {
    /// For each blob path include the remote BlobEntry (oid/size). Paths are
    /// stored relative to the indexed root.
    pub blobs: Vec<(PathBuf, BlobEntry)>, // (path, entry) for blobs (sha1)
    /// LFS objects (path + entry where entry.oid is the sha256 oid)
    pub lfs: Vec<(PathBuf, BlobEntry)>,   // (path, entry) for lfs (sha256)
}

/// Build a local index by scanning `root` recursively. The returned Index maps
/// paths relative to `root` to BlobEntry. For regular files we compute the git
/// blob SHA-1 (header "blob {len}\0" + content). If a file appears to be a
/// Git-LFS pointer (starts with "version https://git-lfs.github.com/spec/"),
/// we attempt to parse the `oid sha256:` line and record that SHA-256 as the
/// oid and mark the entry as LFS.
pub fn build_local_index(root: &Path) -> Result<Index, Error> {
    let mut index = Index::new();
    let prefix = b"version https://git-lfs.github.com/spec/";
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_path_buf();

        // Open file and read a small head to detect pointer files efficiently.
        let f = File::open(&path)
            .with_context(|| format!("failed to open file for indexing: {}", path.display()))?;
        let mut reader = BufReader::new(f);
        let mut head = [0u8; 256];
        let n = reader.read(&mut head)?;
        let head_slice = &head[..n];

        if head_slice.starts_with(prefix) {
            // parse pointer content from whole file to be robust
            let mut full = Vec::new();
            reader.read_to_end(&mut full)?;
            let mut content = Vec::from(head_slice);
            content.extend_from_slice(&full);
            // Try to find line "oid sha256:<hex>"
            let lower = content
                .iter()
                .map(|b| if b.is_ascii_uppercase() { b.to_ascii_lowercase() } else { *b })
                .collect::<Vec<u8>>();
            let needle = b"oid sha256:";
            if let Some(pos) = lower.windows(needle.len()).position(|w| w == needle) {
                let start = pos + needle.len();
                let mut end = start;
                while end < lower.len() {
                    let c = lower[end];
                    if c.is_ascii_whitespace() {
                        break;
                    }
                    end += 1;
                }
                let oid_bytes = &content[start..end];
                let oid = String::from_utf8_lossy(oid_bytes).trim().to_ascii_lowercase();
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                index.insert(
                    rel,
                    BlobEntry {
                        oid,
                        size,
                        is_lfs: true,
                    },
                );
                continue;
            }
        }

        // Otherwise compute git blob SHA-1 (streaming)
        let meta = std::fs::metadata(&path)?;
        let file_size = meta.len();
        // Need to reopen to compute full content hash (reader was partially consumed)
        let f2 = File::open(&path)
            .with_context(|| format!("failed to open file for hashing: {}", path.display()))?;
        let mut buf = BufReader::new(f2);
        let mut hasher = Sha1::new();
        let header = format!("blob {}\u{0}", file_size);
        hasher.update(header.as_bytes());
        let mut tmp = [0u8; 8192];
        loop {
            let read = buf.read(&mut tmp)?;
            if read == 0 {
                break;
            }
            hasher.update(&tmp[..read]);
        }
        let result = hasher.finalize();
        let oid = hex::encode(result);
        index.insert(
            rel,
            BlobEntry {
                oid,
                size: file_size,
                is_lfs: false,
            },
        );
    }
    Ok(index)
}

/// Compare local and remote indexes and produce a SyncPlan listing paths that
/// need to be downloaded. If the remote entry is marked as LFS it will be put
/// into the `lfs` list otherwise into `blobs`.
pub fn compare_indexes(local: &Index, remote: &Index) -> SyncPlan {
    let mut plan = SyncPlan::default();
    for (path, remote_entry) in remote.iter() {
        match local.get(path) {
            Some(local_entry) => {
                if local_entry.oid != remote_entry.oid {
                    if remote_entry.is_lfs {
                        plan.lfs.push((path.clone(), remote_entry.clone()));
                    } else {
                        plan.blobs.push((path.clone(), remote_entry.clone()));
                    }
                }
            }
            None => {
                if remote_entry.is_lfs {
                    plan.lfs.push((path.clone(), remote_entry.clone()));
                } else {
                    plan.blobs.push((path.clone(), remote_entry.clone()));
                }
            }
        }
    }
    plan
}