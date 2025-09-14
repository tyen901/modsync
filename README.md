<!--
    README for the modsync project
    ==============================

    This file documents the purpose and usage of the `modsync` application.  It
    covers basic configuration, how to run the program and what the various
    menu actions do.  See the inline comments in the source code for
    additional details on the implementation.
-->

# modsync

`modsync` is a command line tool and text user interface (TUI) for keeping an
Arma 3 modpack in sync with a remote Git repository.  Unlike a traditional
checkout using Git LFS, `modsync` only uses the pointer files stored in the
repository to determine which large files are required; it then downloads
missing or out‑of‑date files into a separate target directory.  The tool also
provides a mechanism for validating your local files, checking for updates and
launching Arma 3 directly into the server specified by the modpack's
`metadata.json` file.

## Concepts and directory layout

Three distinct locations on disk are used by `modsync`:

1. **Remote repository** – A public Git repository containing the modpack
   definition.  The repository includes the usual tree of files but instead
   of the large `.pbo` files it holds small text files known as Git LFS
   pointers.  Each pointer file records the SHA‑256 of the actual file stored
   on an LFS server.  The repository also contains a `metadata.json` file
   with connection details for the game server.

2. **Local clone** – A clone of the remote repository stored on your
   machine.  This directory mirrors the tree of the remote repository but
   does **not** checkout the LFS objects.  It acts as the reference for
   checking whether new versions of the modpack are available.  The location
   of this clone is set in the configuration file (`local_repo_path`).

3. **Target mod directory** – The folder that holds the real `.pbo` files
   required by Arma 3.  When `modsync` runs a sync operation it scans the
   local clone for pointer files and compares the embedded SHA against
   existing files in the target directory.  If a file is missing or has a
   mismatching hash it is downloaded (the current implementation creates an
   empty placeholder; you would replace this with an actual LFS download).

The first time you run `modsync`, if `config.txt` does not exist next to the
executable the application will create a default `config.txt` using TOML
syntax. After that the application will not overwrite an existing
`config.txt`. Edit this file to set `repo_url` to the URL of your modpack
repository. The paths for the local clone and target directory are
initialised with sensible defaults but can be changed as required.

## Building the project

This project is written in Rust and uses the 2021 edition.  To build the
binary yourself you will need a recent Rust toolchain.  Clone the project
and run the usual Cargo commands:

```sh
git clone https://example.com/modsync.git
cd modsync
cargo build --release
```

The resulting executable can be found at `target/release/modsync`.

## Running the application

<!--
      README for the modsync project
      ==============================

      This file documents the purpose and usage of the `modsync` application.
-->

# modsync

`modsync` is a small Rust application that provides a terminal user
interface (TUI) for synchronising an Arma 3 modpack from a Git repository.
The repository contains Git LFS pointer files instead of large binaries; the
application reads those pointers, compares the referenced SHA‑256 values
against files in a separate target directory and downloads or copies files
when required.

## Concepts and directory layout

modsync uses three locations on disk:

1. Remote repository – a public Git repository containing the modpack
    metadata and Git LFS pointer files. Optionally the repository can include
    a `metadata.json` file describing the Arma server to join.

2. Local clone – a local clone of the remote repository used as read‑only
    metadata. The app clones the repository if `local_repo_path` does not
    exist and performs `fetch` to update it.

3. Target mod directory – the directory containing the real mod files
    (the `.pbo` files). Pointer SHAs read from the repository are compared
    against files in this directory; missing or mismatching files are
    downloaded (or created as placeholders).

On first run the application will create a `config.txt` in the same
directory as the executable if none exists. The file is TOML formatted and
is intended to be edited by the user to set `repo_url` and any paths you
wish to override.

## How syncing and validation work

- Sync: the app walks the files in the local clone. For each file it
   either parses a Git LFS pointer (extracting the SHA‑256) or treats it as
   a regular file. For pointer files the SHA is compared against the SHA of
   the file in `target_mod_dir` at the same relative path. If the file is
   missing or the hashes differ the downloader is invoked. For non‑pointer
   files the file is copied into the target when it differs.

- Validation: similar to sync but read‑only — it returns a list of files in
   the repository whose corresponding files in the target directory are
   missing or whose SHA‑256 does not match.

### LFS downloading behaviour

The downloader implements a single strategy: it uses the Git LFS batch
API exposed by Azure DevOps / VisualStudio-style remotes. When the
repository's origin remote indicates an Azure/VisualStudio host the
application will POST to `.../info/lfs/objects/batch` to obtain a
download action and then fetch the object bytes from the returned
href. Authentication can be supplied via the repository provider (for
example a Personal Access Token) when required.

There is no environment-variable-based fallback. If your hosting
provider exposes a different LFS API (for example GitHub or GitLab) you
should implement a provider-specific downloader that calls the
appropriate endpoints.

## Detecting and launching Arma 3

The configuration may include an `arma_executable` path. If not set the
binary checks the `ARMA3_PATH` environment variable and a few common
installation paths (typical Steam locations on Windows and common Proton
paths on Linux). When launching the game `modsync` passes `-connect=<addr>`,
`-port=<port>` and an optional `-password=<password>` according to the
`metadata.json` fields.

Example `metadata.json`

Below is a minimal example of a `metadata.json` file that you can place at
the root of the modpack repository. `modsync` will read this file when you
choose the "Join Server" action and use the fields to build the Arma
command line.

```json
{
   "address": "arma.example.com",
   "port": 2302,
   "password": "secretpass"
}
```

The `password` field is optional; if omitted no `-password` flag will be
passed to the game.

## TUI and menu actions

The application uses `ratatui` + `crossterm` to present a simple vertical
menu and a log pane. Menu actions are:

- Sync Modpack — clone/open repo, fetch updates and synchronise files into
   the `target_mod_dir`.
- Validate Files — scan and report missing/mismatched files without
   modifying anything.
- Check Updates — fetch the repository and compare HEAD OIDs to report if
   an update is available.
- Join Server — read `metadata.json`, detect or use the configured Arma
   executable and launch the game with connection flags.
- Quit — save config and exit.

Navigation: arrow keys, Enter to execute a menu entry, and `q` to quit.

## Build, run and tests

Requirements: Rust toolchain with Cargo.

Build:

```pwsh
cargo build --release
```

Run (debug):

```pwsh
cargo run
```

Run tests:

```pwsh
cargo test
```

There is a unit test that runs a tiny HTTP server and verifies the HTTP
downloader round‑trip by simulating the Git LFS batch API. The test uses
the fixture at `tests/fixtures/test_blob.bin`.

## Notes and development

-- LFS: to make the tool download real objects, implement a downloader
   that integrates with your hosting provider's LFS API (the project
   currently implements Azure/VisualStudio batch API support). Replace
   or extend the downloader to support other providers if required.
- The repository operations rely on the `git2` crate. The tool clones if
   necessary and uses `fetch` (it does not automatically merge or checkout
   remote branches into the working tree).
- The project's important crates include `git2`, `walkdir`, `sha2`, `hex`,
   `ratatui`, `crossterm`, `directories` and `anyhow`.

Future work:

- Implement a production LFS fetcher and progress reporting for long
   operations in the UI.
- Add in‑UI configuration editing and more detailed logs/status for
   operations.
