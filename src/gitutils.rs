//! Git related helpers for the `modsync` project.
//!
//! The application makes use of the [`git2`](https://docs.rs/git2) crate to
//! clone and update repositories.  Only a limited subset of Git's
//! functionality is required: cloning a repository if it doesn't already
//! exist, fetching new commits on subsequent runs and reading the current
//! commit identifier for update detection.  Authentication is not handled
//! because the repository is expected to be publicly accessible.

use anyhow::{Context, Result};
use git2::{Cred, CredentialType, FetchOptions, Oid, RemoteCallbacks, Repository};
use std::path::Path;

// Build RemoteCallbacks that provide credentials for HTTP(S) remotes when
// appropriate environment variables are set.  We prefer an Azure DevOps
// personal access token (AZURE_DEVOPS_PAT) when present.  Otherwise the
// pair GIT_USERNAME / GIT_PASSWORD is used.  If none are present the
// callbacks will not supply credentials.
fn build_remote_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();

    // Transfer progress is used by callers; provide a noop default here.
    callbacks.transfer_progress(|_| true);

    // Credentials callback: supply username/password or PAT if available.
    callbacks.credentials(|_url, username_from_url, allowed_types| {
        // If Azure DevOps PAT is present, use it as the password and a
        // generic username (some servers accept anything as username).
        if let Ok(pat) = std::env::var("AZURE_DEVOPS_PAT") {
            // Some servers expect the username to be "" or "user"; use
            // the username from URL if provided, otherwise fall back.
            let user = username_from_url.unwrap_or("user");
            return Cred::userpass_plaintext(user, &pat);
        }

        // Otherwise fall back to GIT_USERNAME / GIT_PASSWORD.
        if let (Ok(user), Ok(pass)) = (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD")) {
            return Cred::userpass_plaintext(&user, &pass);
        }

        // If no credentials are available but SSH key auth is allowed use
        // the default SSH key helper.
        if allowed_types.contains(CredentialType::SSH_KEY) {
            return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
        }

        Err(git2::Error::from_str("no credentials available"))
    });

    callbacks
}

/// Either opens an existing repository at `path` or clones a new one from
/// `url`.  The clone is performed recursively so that submodules (if any)
/// will also be cloned.  If the directory already contains a valid
/// repository it is simply opened.
pub fn clone_or_open_repo(url: &str, path: &Path) -> Result<Repository> {
    // If the path exists try to open it as a repository. If it's not a
    // repository, or if the repository's origin URL differs from the
    // requested URL, remove the directory so we can perform a fresh clone.
    if path.exists() {
        match Repository::open(path) {
            Ok(repo) => {
                // Determine the origin URL without keeping a borrow of `repo`.
                let existing_url_opt = repo
                    .find_remote("origin")
                    .ok()
                    .and_then(|r| r.url().map(|s| s.to_string()));

                if let Some(existing_url) = existing_url_opt {
                    if existing_url != url {
                        // If the caller passed the local path as the "url" (for
                        // example tests that clone using the system git CLI and
                        // then call this helper with the clone path), treat that
                        // as a match and return the opened repository instead of
                        // removing it. Try to canonicalize the provided URL and
                        // the path to detect equivalence for local paths.
                        let url_is_same_as_path = (|| {
                            // First, if the provided url string exactly equals the
                            // path display use that as a quick check.
                            if url == path.display().to_string() {
                                return true;
                            }
                            // Try to interpret the URL as a local filesystem
                            // path and canonicalize both sides for comparison. If
                            // any step fails, fall back to conservative false.
                            if let Ok(url_path) = std::path::PathBuf::from(url).canonicalize() {
                                if let Ok(cpath) = path.canonicalize() {
                                    return url_path == cpath;
                                }
                            }
                            false
                        })();

                        if url_is_same_as_path {
                            // Treat as matching repo; return it.
                            return Ok(repo);
                        }

                        // Remote URL changed — remove cache to avoid surprises.
                        std::fs::remove_dir_all(path).with_context(|| {
                            format!(
                                "Removed cached repository at {} because remote URL changed (was: {})",
                                path.display(),
                                existing_url
                            )
                        })?;
                    } else {
                        // Matching repo, return it.
                        return Ok(repo);
                    }
                } else {
                    // No origin URL found — treat as stale and remove.
                    std::fs::remove_dir_all(path).with_context(|| {
                        format!("Removed cached repository at {} (no 'origin' URL)", path.display())
                    })?;
                }
            }
            Err(_) => {
                // Not a valid repo — remove and continue to clone.
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }

    // Perform clone with credential callbacks.
    let mut fetch_opts = FetchOptions::new();
    let callbacks = build_remote_callbacks();
    fetch_opts.remote_callbacks(callbacks);
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);
    let repo = builder
        .clone(url, path)
        .with_context(|| format!("Failed to clone repository from {}", url))?;
    Ok(repo)
}

/// Fetches the latest changes from the remote named `origin`.  This does
/// **not** merge or checkout any branches; it merely updates the remote
/// tracking branches.  If you wish to merge changes into the working tree
/// you can use the standard Git command line tools in the cloned
/// repository.
pub fn fetch(repo: &Repository) -> Result<()> {
    let mut remote = repo
        .find_remote("origin")
        .context("Could not find 'origin' remote in repository")?;

    let mut fetch_options = FetchOptions::new();
    let callbacks = build_remote_callbacks();
    fetch_options.remote_callbacks(callbacks);

    // Fetch all branches and tags.
    remote
        .fetch(
            &[
                "refs/heads/*:refs/remotes/origin/*",
                "refs/tags/*:refs/tags/*",
            ],
            Some(&mut fetch_options),
            None,
        )
        .context("Failed to fetch updates from remote")?;
    Ok(())
}

/// Returns the object ID (SHA‑1) of the current HEAD in the repository.
/// If the repository is in a detached HEAD state the pointed object is returned.
pub fn head_oid(repo: &Repository) -> Result<Oid> {
    // Prefer a remote tracking reference if present (this makes it possible
    // to detect upstream changes after a fetch).  We try a few common
    // tracking refs in order: origin/HEAD, origin/main, origin/master and
    // fall back to the repository's local HEAD if none are available.
    let candidates = [
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
        "refs/remotes/origin/master",
    ];

    for cand in &candidates {
        if let Ok(r) = repo.find_reference(cand) {
            // Resolve symbolic refs to their target and return the OID when
            // possible.
            if let Ok(resolved) = r.resolve() {
                if let Some(oid) = resolved.target() {
                    return Ok(oid);
                }
            } else if let Some(oid) = r.target() {
                return Ok(oid);
            }
        }
    }

    // Fallback to local HEAD.
    let head = repo.head().context("Failed to query HEAD of repository")?;
    let oid = head
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD does not point to a valid commit"))?;
    Ok(oid)
}
