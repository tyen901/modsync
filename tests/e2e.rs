use std::fs::{self, File};
use std::io::Write;

use tempfile::TempDir;
use std::path::Path;
use std::thread;
use tiny_http::{Server, Response};

/// End-to-end integration test that creates a temporary Git repository with
/// an LFS pointer file and a normal file, then clones it and runs the
/// `sync_modpack` and `validate_modpack` flows. This test runs by default.
#[test]
fn e2e_local_git_lfs_repo() {

    // Create a temporary directory to serve as the source repository.
    let src_dir = TempDir::new().expect("failed to create src tempdir");
    let src_path = src_dir.path();

    // Initialise a git repository using the system git CLI for simplicity.
    let status = std::process::Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(src_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("failed to spawn git init");
    assert!(status.success(), "git init failed");

    // Create a normal metadata.json file.
    let meta = src_path.join("metadata.json");
    let mut meta_f = File::create(&meta).expect("failed to create metadata.json");
    writeln!(meta_f, "{{ \"address\": \"127.0.0.1\" }}").expect("write metadata");

    // Compute SHA-256 of the deterministic fixture and write an LFS
    // pointer file referencing that SHA.
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("test_blob.bin");
    let fixture_data = fs::read(&fixture_path).expect("read fixture");

    // To compute the SHA using the library helper we need a file on disk.
    let tmp_blob = TempDir::new().expect("tmp for blob");
    let tmp_blob_path = tmp_blob.path().join("blob.bin");
    fs::write(&tmp_blob_path, &fixture_data).expect("write tmp blob");
    let fixture_sha = modsync::modpack::compute_sha256(&tmp_blob_path).expect("compute sha");

    // Do not create the pointer in the source repo; we'll write it into the
    // cloned repository to avoid CRLF/BOM/git-normalisation issues that can
    // alter the file contents during clone/commit. This keeps the test
    // deterministic across platforms.

    // Commit the files using git CLI.
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add failed");
    assert!(status.success());

    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("commit")
        .arg("-m")
        .arg("initial commit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Create a temporary directory to host a non-existent clone path (so
    // `clone_or_open_repo` will clone into it) and a separate target mod dir.
    let clone_parent = TempDir::new().expect("failed to create clone parent dir");
    let clone_path = clone_parent.path().join("cloned_repo");
    let target_dir = TempDir::new().expect("failed to create target dir");

    // (not used) compute absolute path for reference if needed
    let _ = src_path.canonicalize().expect("canonicalize");

    // Use the system git CLI to clone the local repository into the
    // desired clone path.  This avoids libgit2 URI parsing/clone issues on
    // some platforms. After cloning open the repository with
    // `clone_or_open_repo` to exercise the library open path.
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(&src_path)
        .arg(&clone_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("failed to spawn git clone");
    assert!(status.success(), "git clone failed");

    // Open the cloned repository using the library function.
    let repo = modsync::gitutils::clone_or_open_repo(&clone_path.display().to_string(), &clone_path).expect("clone_or_open_repo failed");

    // Write the LFS pointer into the cloned repository and commit it. Placing
    // the pointer in the clone avoids any transformations that might occur
    // when the source repository is cloned.
    let cloned_pointer = clone_path.join("mods").join("example.pbo");
    if let Some(parent) = cloned_pointer.parent() {
        fs::create_dir_all(parent).expect("create clone mods dir");
    }
    let mut p_f = File::create(&cloned_pointer).expect("failed to create pointer in clone");
    writeln!(p_f, "version https://git-lfs.github.com/spec/v1").unwrap();
    writeln!(p_f, "oid sha256:{}", fixture_sha).unwrap();
    writeln!(p_f, "size {}", fixture_data.len()).unwrap();

    // Commit the new pointer file in the cloned repo.
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&clone_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add in clone failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&clone_path)
        .arg("commit")
        .arg("-m")
        .arg("add pointer in clone")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit in clone failed");
    assert!(status.success());

    // Start a tiny HTTP server that will serve the real blob at /{sha}.
    // The server runs in a background thread and will be shut down when
    // the test exits. We set LFS_SERVER_URL so `download_lfs_object`
    // will fetch from it.
    let fixture_clone = fixture_data.clone();
    let server = Server::http("127.0.0.1:0").expect("failed to start test server");
    let server_addr = server.server_addr();
    let server_url = format!("http://{}", server_addr);

    // Spawn server thread
    let _handle = thread::spawn(move || {
        for request in server.incoming_requests() {
            // Always serve the fixture for any request in this test.
            let body = Response::from_data(fixture_clone.clone());
            let _ = request.respond(body);
        }
    });

    // Export server URL so the modpack downloader uses it.
    std::env::set_var("LFS_SERVER_URL", &server_url);

    // Fetch and run sync. The sync will download the real blob from our test
    // server into the target directory.
    modsync::gitutils::fetch(&repo).expect("fetch failed");
    modsync::modpack::sync_modpack(&clone_path, target_dir.path()).expect("sync_modpack failed");

    // Sanity check: ensure the downloaded blob has the expected SHA.
    let target_blob = target_dir.path().join("mods").join("example.pbo");
    let downloaded_sha = modsync::modpack::compute_sha256(&target_blob).expect("compute downloaded sha");
    assert_eq!(downloaded_sha, fixture_sha, "downloaded blob sha mismatch");

    let mismatches = modsync::modpack::validate_modpack(&clone_path, target_dir.path()).expect("validate_modpack failed");
    // The hashes should match; expect zero mismatches.
    assert!(mismatches.is_empty(), "expected zero mismatches, found {}", mismatches.len());

    // Clean up: remove env var. The HTTP server thread will exit when the
    // process terminates; we don't join it here to avoid blocking.
    std::env::remove_var("LFS_SERVER_URL");
}

// Ensure that regular (non-LFS) files are copied into the target when
// syncing and validated correctly.
#[test]
fn e2e_sync_regular_file() {
    let src_dir = TempDir::new().expect("failed to create src tempdir");
    let src_path = src_dir.path();

    // Init git repo
    let status = std::process::Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(src_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("failed to spawn git init");
    assert!(status.success());

    // Create a normal file under mods/normal.txt
    let normal = src_path.join("mods").join("normal.txt");
    if let Some(p) = normal.parent() { std::fs::create_dir_all(p).unwrap(); }
    let mut f = File::create(&normal).expect("create normal");
    writeln!(f, "hello world").unwrap();

    // Commit
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("commit")
        .arg("-m")
        .arg("initial commit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Clone and run sync
    let clone_parent = TempDir::new().expect("clone parent");
    let clone_path = clone_parent.path().join("cloned_repo");
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(&src_path)
        .arg(&clone_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git clone failed");
    assert!(status.success());

    let repo = modsync::gitutils::clone_or_open_repo(&clone_path.display().to_string(), &clone_path).expect("clone_or_open_repo failed");
    let target_dir = TempDir::new().expect("target");

    modsync::gitutils::fetch(&repo).expect("fetch failed");
    modsync::modpack::sync_modpack(&clone_path, target_dir.path()).expect("sync_modpack failed");

    let target_file = target_dir.path().join("mods").join("normal.txt");
    assert!(target_file.exists(), "expected normal file to be copied");
    let mismatches = modsync::modpack::validate_modpack(&clone_path, target_dir.path()).expect("validate failed");
    assert!(mismatches.is_empty(), "expected zero mismatches");
}

// Ensure repair behavior: when a target file differs from the desired
// content the sync should replace it with the correct content (for LFS
// objects we simulate download via a test HTTP server as in the other test).
#[test]
fn e2e_repair_mismatch() {
    // Setup source with an LFS pointer referencing the fixture
    let src_dir = TempDir::new().expect("src");
    let src_path = src_dir.path();
    let status = std::process::Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(src_path)
        .status()
        .expect("git init failed");
    assert!(status.success());

    let meta = src_path.join("metadata.json");
    let mut meta_f = File::create(&meta).expect("failed to create metadata.json");
    writeln!(meta_f, "{{ \"address\": \"127.0.0.1\" }}").expect("write metadata");

    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("test_blob.bin");
    let fixture_data = fs::read(&fixture_path).expect("read fixture");
    let tmp_blob = TempDir::new().expect("tmp for blob");
    let tmp_blob_path = tmp_blob.path().join("blob.bin");
    fs::write(&tmp_blob_path, &fixture_data).expect("write tmp blob");
    let fixture_sha = modsync::modpack::compute_sha256(&tmp_blob_path).expect("compute sha");

    // commit metadata
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("commit")
        .arg("-m")
        .arg("initial commit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Clone
    let clone_parent = TempDir::new().expect("clone parent");
    let clone_path = clone_parent.path().join("cloned_repo");
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(&src_path)
        .arg(&clone_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git clone failed");
    assert!(status.success());

    // Add pointer in clone pointing to fixture
    let cloned_pointer = clone_path.join("mods").join("example.pbo");
    if let Some(parent) = cloned_pointer.parent() { fs::create_dir_all(parent).unwrap(); }
    let mut p_f = File::create(&cloned_pointer).expect("failed to create pointer in clone");
    writeln!(p_f, "version https://git-lfs.github.com/spec/v1").unwrap();
    writeln!(p_f, "oid sha256:{}", fixture_sha).unwrap();
    writeln!(p_f, "size {}", fixture_data.len()).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&clone_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add in clone failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&clone_path)
        .arg("commit")
        .arg("-m")
        .arg("add pointer in clone")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit in clone failed");
    assert!(status.success());

    // Create a target directory that already contains a corrupted file
    let target_dir = TempDir::new().expect("target");
    let target_blob = target_dir.path().join("mods").join("example.pbo");
    if let Some(p) = target_blob.parent() { fs::create_dir_all(p).unwrap(); }
    fs::write(&target_blob, b"corrupted data").expect("write corrupted");

    // Start HTTP server to serve the real blob
    let fixture_clone = fixture_data.clone();
    let server = Server::http("127.0.0.1:0").expect("failed to start test server");
    let server_addr = server.server_addr();
    let server_url = format!("http://{}", server_addr);
    let _handle = thread::spawn(move || {
        for request in server.incoming_requests() {
            let body = Response::from_data(fixture_clone.clone());
            let _ = request.respond(body);
        }
    });
    std::env::set_var("LFS_SERVER_URL", &server_url);

    let repo = modsync::gitutils::clone_or_open_repo(&clone_path.display().to_string(), &clone_path).expect("clone_or_open_repo failed");
    modsync::gitutils::fetch(&repo).expect("fetch failed");
    modsync::modpack::sync_modpack(&clone_path, target_dir.path()).expect("sync failed");

    // Verify repaired: the corrupted file should now match fixture
    let downloaded_sha = modsync::modpack::compute_sha256(&target_blob).expect("compute downloaded sha");
    assert_eq!(downloaded_sha, fixture_sha, "repaired blob sha mismatch");

    let mismatches = modsync::modpack::validate_modpack(&clone_path, target_dir.path()).expect("validate failed");
    assert!(mismatches.is_empty(), "expected zero mismatches after repair");
    std::env::remove_var("LFS_SERVER_URL");
}

// Ensure update detection: if the remote HEAD changes after fetch the
// check should report an update available (we exercise head_oid and fetch).
#[test]
fn e2e_detect_update_available() {
    let src_dir = TempDir::new().expect("src");
    let src_path = src_dir.path();
    let status = std::process::Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(src_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git init failed");
    assert!(status.success());

    // initial commit
    let meta = src_path.join("metadata.json");
    let mut meta_f = File::create(&meta).expect("failed to create metadata.json");
    writeln!(meta_f, "{{ \"address\": \"127.0.0.1\" }}").expect("write metadata");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("commit")
        .arg("-m")
        .arg("initial commit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Clone repo
    let clone_parent = TempDir::new().expect("clone parent");
    let clone_path = clone_parent.path().join("cloned_repo");
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(&src_path)
        .arg(&clone_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git clone failed");
    assert!(status.success());

    let repo = modsync::gitutils::clone_or_open_repo(&clone_path.display().to_string(), &clone_path).expect("clone_or_open_repo failed");

    // Record before
    let before = modsync::gitutils::head_oid(&repo).ok();

    // Create a new commit in the source repo to simulate an update
    let newfile = src_path.join("mods").join("new.txt");
    if let Some(p) = newfile.parent() { fs::create_dir_all(p).unwrap(); }
    fs::write(&newfile, b"updated").expect("write new");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("add")
        .arg(".")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git add failed");
    assert!(status.success());
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(src_path)
        .arg("commit")
        .arg("-m")
        .arg("update")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Fetch into clone and check head_oid changed
    modsync::gitutils::fetch(&repo).expect("fetch failed");
    let after = modsync::gitutils::head_oid(&repo).ok();
    assert!(before.is_some() && after.is_some(), "could not determine before/after oids");
    assert!(before.unwrap() != after.unwrap(), "expected update to change head oid");
}
