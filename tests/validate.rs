use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use modsync::modpack::{collect_download_items, compute_sha256, parse_lfs_pointer_file, validate_modpack};

#[test]
fn test_validate_modpack_detects_mismatches() {
    let repo_tmp = TempDir::new().expect("repo tmpdir");
    let repo_path = repo_tmp.path();
    let target_tmp = TempDir::new().expect("target tmpdir");
    let target_path = target_tmp.path();

    // Create a normal file that matches
    let a_repo = repo_path.join("a.txt");
    fs::write(&a_repo, b"AAA").expect("write a_repo");
    let a_target = target_path.join("a.txt");
    fs::write(&a_target, b"AAA").expect("write a_target");

    // Create a normal file that mismatches
    let b_repo = repo_path.join("b.txt");
    fs::write(&b_repo, b"BBB").expect("write b_repo");
    let b_target = target_path.join("b.txt");
    fs::write(&b_target, b"ZZZ").expect("write b_target");

    // Create a pointer file referring to a blob; target will contain the blob
    let blob = b"this is blob data";
    let blob_tmp = repo_tmp.path().join("blob.bin");
    fs::write(&blob_tmp, blob).expect("write blob");
    let blob_sha = compute_sha256(&blob_tmp).expect("compute blob sha");

    let c_repo = repo_path.join("c.bin");
    let pointer_contents = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
        blob_sha,
        blob.len()
    );
    fs::write(&c_repo, pointer_contents.as_bytes()).expect("write pointer file");

    // Put the blob into the target path so pointer validates.
    let c_target = target_path.join("c.bin");
    fs::write(&c_target, blob).expect("write target blob");

    let mismatches = validate_modpack(repo_path, target_path).expect("validate");
    let set: HashSet<String> = mismatches
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // b.txt should be mismatched; a.txt and c.bin should be valid
    assert!(set.contains("b.txt"), "expected b.txt to be reported");
    assert!(!set.contains("a.txt"), "a.txt should not be reported");
    assert!(!set.contains("c.bin"), "c.bin should not be reported");
}

#[test]
fn test_collect_download_items_finds_missing_pointer() {
    let repo_tmp = TempDir::new().expect("repo tmpdir");
    let repo_path = repo_tmp.path();
    let target_tmp = TempDir::new().expect("target tmpdir");
    let target_path = target_tmp.path();

    // Create a blob and pointer in repo; do not create target file so it should be collected
    let blob = b"some other blob";
    let blob_tmp = repo_tmp.path().join("blob2.bin");
    fs::write(&blob_tmp, blob).expect("write blob2");
    let blob_sha = compute_sha256(&blob_tmp).expect("compute blob2 sha");

    let d_repo = repo_path.join("d.bin");
    let pointer_contents = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
        blob_sha,
        blob.len()
    );
    fs::write(&d_repo, pointer_contents.as_bytes()).expect("write pointer d");

    let items = collect_download_items(repo_path, target_path).expect("collect");
    assert_eq!(items.len(), 1, "expected one download item");
    let item = &items[0];
    assert_eq!(item.oid, blob_sha, "oid should match blob sha");
    let expected_dest: PathBuf = target_path.join("d.bin");
    assert_eq!(item.dest, expected_dest, "dest path should match");
}

#[test]
fn test_parse_pointer_case_insensitive() {
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("upcase.ptr");
    // Intentionally use uppercase keywords
    let contents = "VERSION https://git-lfs.github.com/spec/v1\nOID SHA256:ABCDEF1234567890\nSIZE 16\n";
    fs::write(&p, contents.as_bytes()).expect("write upcase pointer");
    let parsed = parse_lfs_pointer_file(&p).expect("parse");
    assert!(parsed.is_some(), "expected pointer to be parsed case-insensitively");
    let ptr = parsed.unwrap();
    assert_eq!(ptr.oid, "abcdef1234567890", "oid should be lowercased");
}
