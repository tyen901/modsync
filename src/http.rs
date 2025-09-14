use anyhow::Error;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Clone, Debug)]
pub struct AzureClient {
    pub base_url: Url,
    pub token: Option<String>,
    pub client: Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    pub objectId: Option<String>,
    pub gitObjectType: Option<String>,
    pub path: Option<String>,
    pub isFolder: Option<bool>,
    pub url: Option<String>,
    pub size: Option<u64>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemsResponse {
    pub value: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsObject {
    pub oid: String,
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsBatchRequest {
    pub operation: String,
    pub objects: Vec<LfsObject>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsAction {
    pub href: Option<String>,
    pub header: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsObjectResponse {
    pub oid: String,
    pub size: Option<u64>,
    pub actions: Option<HashMap<String, LfsAction>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LfsBatchResponse {
    pub objects: Vec<LfsObjectResponse>,
}

impl AzureClient {
    /// Create a new AzureClient for the given base URL (e.g. the repository root
    /// API endpoint such as `https://dev.azure.com/{org}/{project}/_apis/git/repositories/{repo}`).
    pub async fn new(base: &str, token: Option<String>) -> Result<Self, Error> {
        let base_url = Url::parse(base)?;
        let client = Client::builder().build()?;
        Ok(AzureClient {
            base_url,
            token,
            client,
        })
    }

    /// List items for a given path at a specific commit. This calls the Azure
    /// Items API with recursionLevel=Full.
    pub async fn list_items_commit(
        &self,
        path: &str,
        commit: &str,
    ) -> Result<ItemsResponse, Error> {
        let base = self.base_url.as_str().trim_end_matches('/');
        let url = format!(
            "{base}/items?path={path}&recursionLevel=Full&includeContentMetadata=true&versionDescriptor.version={commit}&versionDescriptor.versionType=commit&api-version=7.1",
            base = base,
            path = urlencoding::encode(path),
            commit = commit
        );
        let req = self.client.get(&url);
        // If a token is provided we do not assume a specific auth scheme here.
        // For public repos token is usually not required. Callers may extend this.
        if let Some(_) = &self.token {
            // Intentionally not adding auth header by default to keep behaviour simple.
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Azure Items API failed: HTTP {}: {}",
                status,
                String::from_utf8_lossy(&body)
            ));
        }
        let items: ItemsResponse = serde_json::from_slice(&body)?;
        Ok(items)
    }

    /// Fetch a blob by its Git SHA-1 object id. Returns raw bytes of the blob.
    pub async fn get_blob_by_oid(&self, oid: &str) -> Result<Bytes, Error> {
        let base = self.base_url.as_str().trim_end_matches('/');
        let url = format!(
            "{base}/blobs/{oid}?api-version=7.1&$format=octetstream",
            base = base,
            oid = oid
        );
        let req = self.client.get(&url);
        if let Some(_) = &self.token {
            // no-op for now
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Blob GET failed: HTTP {}: {}",
                status,
                String::from_utf8_lossy(&body)
            ));
        }
        Ok(body)
    }

    /// Perform an LFS batch request (download operation). Returns a deserialised response.
    pub async fn lfs_batch(&self, req_body: LfsBatchRequest) -> Result<LfsBatchResponse, Error> {
        let base = self.base_url.as_str().trim_end_matches('/');
        let url = format!("{base}/info/lfs/objects/batch", base = base);
        let req = self
            .client
            .post(&url)
            .header("Accept", "application/vnd.git-lfs+json")
            .header("Content-Type", "application/vnd.git-lfs+json")
            .json(&req_body);
        if let Some(_) = &self.token {
            // no-op
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "LFS batch failed: HTTP {}: {}",
                status,
                String::from_utf8_lossy(&body)
            ));
        }
        let parsed: LfsBatchResponse = serde_json::from_slice(&body)?;
        Ok(parsed)
    }
}
