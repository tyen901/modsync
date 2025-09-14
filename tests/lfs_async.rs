use modsync::lfs::{LfsRequestItem, download_lfs_objects_async};
use modsync::http::AzureClient;
use std::sync::Arc;
use tiny_http::{Server, Response, Header};
use std::thread;

#[tokio::test]
async fn test_parallel_lfs_downloads() {
    // Start tiny_http server on a background thread
    let server = Server::http("0.0.0.0:0").unwrap();
    let addr = server.server_addr();
    let base_url = format!("http://{}", addr);

    // We will serve two endpoints:
    // - POST /info/lfs/objects/batch -> returns LFS batch JSON with download hrefs
    // - GET /objects/{name} -> returns bytes for that object
    let srv = Arc::new(server);
    let srv_clone = srv.clone();
    let server_base_for_thread = base_url.clone();
    thread::spawn(move || {
        for request in srv_clone.incoming_requests() {
            let url = request.url().to_string();
            if url == "/info/lfs/objects/batch" && request.method() == &tiny_http::Method::Post {
                // respond with a batch JSON that points to /objects/obj1 and /objects/obj2
                let body = r#"{ "objects": [ { "oid": "obj1", "size": 8, "actions": { "download": { "href": "REPLACE_OBJ1" } } }, { "oid": "obj2", "size": 9, "actions": { "download": { "href": "REPLACE_OBJ2" } } } ] }"#;
                // replace placeholders with full URLs
                let b = body.replace("REPLACE_OBJ1", &format!("{}/objects/obj1", server_base_for_thread)).replace("REPLACE_OBJ2", &format!("{}/objects/obj2", server_base_for_thread));
                let resp = Response::from_string(b).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/vnd.git-lfs+json"[..]).unwrap());
                let _ = request.respond(resp);
                continue;
            }
            if url.starts_with("/objects/") {
                // return deterministic content based on object name
                let name = url.trim_start_matches("/objects/");
                let body = match name {
                    "obj1" => "contents1",
                    "obj2" => "contents22",
                    _ => "x",
                };
                let resp = Response::from_string(body.to_string());
                let _ = request.respond(resp);
                continue;
            }

            // Default
            let resp = Response::from_string("not found").with_status_code(404);
            let _ = request.respond(resp);
        }
    });

    // Create an AzureClient pointing at the tiny_http server base
    let client = AzureClient::new(&base_url, None).await.unwrap();

    // Prepare two LFS request items that map to paths a.txt and b.txt
    let p1 = std::path::PathBuf::from("a.txt");
    let p2 = std::path::PathBuf::from("b.txt");
    let items = vec![
        LfsRequestItem {
            oid: "obj1".to_string(),
            size: Some(8),
            paths: vec![p1.clone()],
            repo_remote: None,
        },
        LfsRequestItem {
            oid: "obj2".to_string(),
            size: Some(9),
            paths: vec![p2.clone()],
            repo_remote: None,
        },
    ];

    let out_dir = tempfile::tempdir().unwrap();
    let res = download_lfs_objects_async(&client, items, out_dir.path(), 2).await;
    assert!(res.is_ok());
    let summary = res.unwrap();
    assert_eq!(summary.files_done, 2);

    // Verify files exist and have expected contents
    let a = std::fs::read_to_string(out_dir.path().join("a.txt")).unwrap();
    let b = std::fs::read_to_string(out_dir.path().join("b.txt")).unwrap();
    assert_eq!(a, "contents1");
    assert_eq!(b, "contents22");
}
