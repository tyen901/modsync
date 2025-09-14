Synchronizing a Local Folder with an Azure DevOps Git LFS Repository
1. Retrieve the Remote File Tree at a Specific Commit
Begin by fetching the full file tree (all files and directories) of the repository at the target commit using Azure DevOps’s Git REST API. Azure DevOps provides an Items API that can list repository contents at a given commit without needing a local clone. You can call:
GET https://dev.azure.com/{organization}/{project}/_apis/git/repositories/{repo}/items?path={path}&recursionLevel=Full&includeContentMetadata=true&versionDescriptor.version={commitSha}&versionDescriptor.versionType=commit&api-version=7.1
In this request: - Set path=/ (the root path) and recursionLevel=Full to retrieve all files recursively[1]. - Supply the commit SHA in versionDescriptor.version (and versionType=commit) to get the state at that commit[2].
The response will be JSON listing each item (file or folder) in the commit. For example, each file entry includes its path and a Git object ID (SHA-1) if it’s a blob[3]:
{
  "objectId": "61a86fdaa79e5c6f5fb6e4026508489feb6ed92c",
  "gitObjectType": "blob",
  "path": "/path/to/file.ext",
  "isFolder": false,
  "url": ".../items/path/to/file.ext?versionType=Commit&..."}
Key points: The objectId is the SHA-1 of the file’s blob content in that commit. This listing gives you a complete snapshot of remote filenames and their blob hashes at the specified commit. No local repo is needed – you’re using Azure’s REST API to enumerate the commit contents.
🔹 Note: Azure DevOps also supports a $format=zip option to download the entire repository content as a ZIP for a given commit[4], but here we only need metadata first to decide which files to download.
2. Identify LFS-Tracked Files and Retrieve Pointer Data
Next, determine which files in the list are Git LFS tracked files. In Git LFS, the repository doesn’t store the actual content of large files – instead, it stores small pointer files that reference the binary content in separate storage. You need to find those pointer files and extract their info (OID and size).
Approach A: Use .gitattributes patterns. If the repo uses Git LFS, it should have a .gitattributes file listing patterns of files managed by LFS (for example, lines like *.psd filter=lfs diff=lfs merge=lfs for each file type). You can download the .gitattributes file (it’s usually at the repo root) via the same Items API or the blob API. Check for any entries with filter=lfs – those file patterns indicate LFS content. Then, for each file in the commit snapshot, if its path matches an LFS pattern, it’s stored as an LFS pointer.
Approach B: Detect by pointer file format. You can also identify LFS files by retrieving a file’s content and seeing if it matches the pointer format. An LFS pointer file is a tiny text file (usually 100-200 bytes) with content like:
version https://git-lfs.github.com/spec/v1
oid sha256:<64-character SHA256 hash>
size <file size>
For example[5]:
version https://git-lfs.github.com/spec/v1  
oid sha256:a747cfbbef63fc0a3f5ffca332ae486ee7bf77c1d1b9b2de02e261ef97d085fe  
size 4923023  
If you did not already use .gitattributes, you can programmatically fetch the content of any suspiciously large file or known binary type from the commit to check if it’s an LFS pointer. However, using .gitattributes is more efficient to avoid fetching file content unnecessarily.
Retrieving pointer files: For each identified LFS-tracked file, use the Azure DevOps API to get its pointer file content. This can be done by calling the Items API or the Blobs API for that file’s object ID. For example, using the Blob API by ID:
GET https://dev.azure.com/{org}/{project}/_apis/git/repositories/{repo}/blobs/{objectId}?$format=text&api-version=7.1
This returns the pointer file content as text (since pointer files are small). You could also use includeContent=true on the Items API for that file[6]. Ensure you do not set resolveLfs=true on these calls, because we want the pointer itself, not the actual large file content at this stage.
From each pointer file, parse out the oid (SHA-256) and size values. These will be used to download the real content from LFS and to verify local files.
3. Compare Local Files to Remote Commit
With the remote file list and LFS pointer info in hand, the tool can now determine which files to update or download. The goal is to identify files that are missing or different in the local folder compared to the specific commit.
a. Build a local file index: Traverse the local folder, listing all files (with their paths relative to the sync root). For each local file, compute its hash for comparison:
For normal files (not LFS): Compute the Git blob SHA-1 of the file’s content. This is the same hash that Azure DevOps provided as the objectId. Note: The Git blob SHA is calculated by prepending the content with a blob <size>\0 header and then SHA-1 hashing. You can replicate this in Rust (e.g., read file bytes, compute SHA1 of b"blob <len>\0" + content). The computed hash can be directly compared to the remote objectId for that path[7]. If they match, the file is identical to the commit version; if not, it has changed.
For LFS files: The remote commit doesn’t store the actual content’s SHA-1, so instead compare the SHA-256 OID from the pointer to the local file’s SHA-256. Compute the local file’s SHA-256 (since the local file is presumably the full binary). Compare it to the oid sha256: value from the pointer. If they match, the local file is up-to-date; if not (or if the file is missing), you will need to download the correct version.
It’s wise to also compare file sizes as a quick check. For non-LFS files, if the size differs from the remote blob’s size (you can get blob size from Azure if needed[8]), it’s definitely changed. For LFS files, the pointer’s size field tells the expected content length[5]; if the local file’s size differs, it’s out of sync. Size mismatches can save you from hashing in obvious cases. However, always use the cryptographic hash for final verification to avoid false negatives.
After this step, you will have a list of files that are either missing locally or have different content than the commit (according to hash comparison).
4. Download Missing or Outdated Files from Azure DevOps
Now, perform targeted downloads for each file that needs to be “healed” (added or updated). Use Azure DevOps’s Git blob API for regular files and the Git LFS Batch API for LFS files:
Normal Git-tracked files (blobs): Use the Blob REST API to fetch the file content by its blob SHA-1. Azure DevOps offers an endpoint to retrieve a blob directly by object ID. For example:
GET https://dev.azure.com/{org}/{project}/_apis/git/repositories/{repo}/blobs/{sha1}?api-version=7.1&$format=octetstream
Setting $format=octetstream yields the raw file bytes (binary stream)[9]. The response will be the file content, which you can then write to the corresponding local path (overwriting or creating the file). This avoids having to deal with Git trees or checkouts – you’re pulling the blob directly from the repository.
🔹 Tip: The blob API respects the commit history of the repo. As long as the blob sha1 exists in the repository (which it will if it was in that commit), this call returns it. You don’t need to specify the commit again here, since the blob hash uniquely identifies the content.
LFS-tracked files: For large files, use the Git LFS protocol to fetch content via the Azure DevOps LFS service. Azure DevOps supports the standard Git LFS batch API at the repository’s LFS endpoint. The endpoint is of the form:

POST https://dev.azure.com/{organization}/{project}/_git/{repo}.git/info/lfs/objects/batch
For each LFS file (or you can batch multiple files in one request), send a JSON request with an operation of "download" and the list of object OIDs and sizes. For example, to download one object[10]:
POST .../objects/batch 
Content-Type: application/vnd.git-lfs+json
Accept: application/vnd.git-lfs+json

{
  "operation": "download",
  "transfer": ["basic"],
  "objects": [
    { "oid": "<sha256oid>", "size": <filesize> }
  ]
}
Azure DevOps will respond with download instructions for each object. The response includes a "download" action with a URL (and possibly headers or a token) to fetch the actual file content[11]. For example, the response might look like:
{
  "objects": [
    {
      "oid": "<sha256oid>",
      "size": 123456789,
      "actions": {
        "download": {
          "href": "https://<storage-url>/.../<oid>?token=...",
          "expires_at": "2025-01-01T12:34:56Z",
          "expires_in": 3600
        }
      }
    }
  ]
}
Here, "href" is a URL to download the binary content (often an Azure Blob Storage or CDN link), and "expires_in" tells how long the link is valid. To retrieve the file: perform an HTTP GET to the href URL. If the response also provides custom HTTP headers (like an auth token in "header" fields), include those in your GET request. In many cases for public repos, the href may contain a time-limited token in the URL itself, so you can download without additional auth.
When you GET that href, the response will be the raw binary content of the large file. Stream this to a file on disk (to avoid loading huge files entirely in memory). Save it to the correct local path.
Finally, after downloading, you can verify the file’s integrity by recomputing its SHA-256 and confirming it matches the OID from the pointer (and perhaps its size).
ℹ️ Note: Azure DevOps does not provide a special public REST API for LFS content outside the standard Git LFS protocol. The above process (pointer parsing → LFS batch request → file download) is the intended way to fetch LFS files programmatically[12][13]. Ensure your tool sends the correct Content-Type and Accept headers as shown, since the LFS server expects the batch request in a specific JSON format.
Authentication: Since we assume a public repo, you don’t need authentication tokens for Azure DevOps API calls or LFS downloads. Azure DevOps will either allow anonymous access or include a token in the href. If you were accessing a private Azure DevOps repo, you would need to provide a Personal Access Token (PAT) or use OAuth for the REST API, and the LFS batch response would include an auth header (e.g., "Authorization": "Bearer <token>") to use when fetching the file[14]. In our public scenario, this is not required.
5. Implementing the Sync Logic in Rust
When building this as a Rust application, consider the following tips and libraries:
HTTP Requests: Use a robust HTTP client like reqwest (which supports async and integrates with tokio if needed) or ureq for synchronous calls. These will handle HTTPS and allow you to easily add headers for the REST and LFS calls. For Azure DevOps’s API, no auth header is needed for public repos; otherwise, you’d include a Basic auth with PAT.
Parsing JSON: The Azure DevOps responses and LFS batch responses are JSON. Leverage serde with serde_json to define structs for the parts you need (e.g., a struct for the value array of items with fields like path and objectId, and structs for the LFS batch request/response). This makes it easy to deserialize the JSON into Rust structs for processing.
Hashing: To compare file content, use crates like sha1 for SHA-1 and either sha2 or RustCrypto’s digest for SHA-256. For SHA-1, remember to prefix the content with the Git blob header before hashing. You can do this by writing a small helper that takes a byte slice and returns the Git blob SHA-1:
use sha1::Sha1;
fn git_blob_sha1(content: &[u8]) -> String {
    let mut hasher = Sha1::new();
    // Write "blob {length}\0" header
    hasher.update(format!("blob {}\u{0}", content.len()).as_bytes());
    hasher.update(content);
    let hash_bytes = hasher.finalize();
    hex::encode(hash_bytes)  // returns hex string of SHA1
}
For SHA-256 of LFS files, a standard SHA-256 hash of the file bytes should match the LFS OID.
Disk I/O and Streaming: Use buffered file reading (e.g., via std::fs::File and std::io::BufReader) to compute hashes on large files without loading everything in memory. Similarly, when downloading large files via reqwest, you can stream the response to a file (using reqwest::blocking::get(...)? and iterating over bytes or using bytes_stream in async). This prevents high memory usage for large content.
Comparisons and Efficiency: You can optimize the sync by first comparing file sizes before doing hashes. For example, if a local file’s size doesn’t match the remote’s size (for non-LFS files you would need to get the size via an extra API call like the Blob metadata[8] or by enabling includeContentMetadata and checking the size if available), you can mark it as changed immediately. For LFS, the pointer size gives the exact expected size[5], so a quick check against local file length can avoid unnecessary hashing of huge files when they clearly differ in size.
Batching and Parallelism: If there are many files to download, you can batch multiple LFS OIDs in one /objects/batch request (the API accepts an array of objects). Azure will return a list of download URLs for all of them[13]. You can then download them possibly in parallel. Be mindful of not overwhelming the network or Azure DevOps – a small threadpool or async task group can help manage parallel downloads. For normal blobs, you might also fetch several concurrently (since each is an independent GET). Use tokio::spawn or futures::stream::FuturesUnordered for concurrency if using async Rust.
Error Handling and Retries: Implement robust error handling. Network calls can fail; for example, a batch request might occasionally return an expired token if not used quickly, in which case you can retry the batch request to get a fresh URL. For idempotency, you might download to a temp file and then rename to the final name once fully downloaded and verified, to avoid partial files if interrupted.
By following these steps, your Rust tool will: (1) fetch the remote commit’s file list via Azure DevOps REST, (2) identify LFS files and get their pointers, (3) compare against local files using hashes/sizes, and (4) download only the necessary files using the proper Git LFS and blob endpoints. This achieves a synchronization (“heal”) without ever needing to do a full git clone or have a local Git repository, working entirely through Azure DevOps’s HTTP APIs and the LFS protocol.
Sources: Azure DevOps REST API documentation for listing items and blobs[1][3][9], Azure Repos guidance on Git LFS pointers[5], and Git LFS API usage examples[10][11] which illustrate the batch request/response flow. Enjoy building your custom Rust sync tool!

[1] [2] [3] [4] [6] Items - Get - REST API (Azure DevOps Git) | Microsoft Learn
https://learn.microsoft.com/en-us/rest/api/azure/devops/git/items/get?view=azure-devops-rest-7.1
[5] Work with large files in your Git repo - Azure Repos | Microsoft Learn
https://learn.microsoft.com/en-us/azure/devops/repos/git/manage-large-files?view=azure-devops
[7] [8] [9] Blobs - Get Blob - REST API (Azure DevOps Git) | Microsoft Learn
https://learn.microsoft.com/en-us/rest/api/azure/devops/git/blobs/get-blob?view=azure-devops-rest-7.1
[10] [11] [12] [13] [14] How to download GIT LFS files · GitHub
https://gist.github.com/fkraeutli/66fa741d9a8c2a6a238a01d17ed0edc5