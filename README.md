# mod_sync

A small tool to sync a local folder with an Azure DevOps Git LFS repo.

## Usage

Purpose: "Sync local folder to an Azure DevOps repo commit using Azure DevOps REST + Git LFS batch; no local git clone required."

Usage: "Usage: mod_sync --organization ORG --project PROJECT --repo REPO --commit COMMIT_SHA --path ./local_folder"

Brief steps the tool performs: "1) build local index (git-blob SHA1 for normal files, SHA256 for LFS), 2) fetch remote commit items, 3) compute SyncPlan, 4) download missing/outdated blobs and LFS objects, writing atomically."

Note about auth: "For private repos, set env var MOD_SYNC_TOKEN or pass --token."
