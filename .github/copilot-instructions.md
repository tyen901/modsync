## Quick context for AI coding agents

This repository implements a small Rust TUI tool (`modsync`) that
synchronises an Arma 3 modpack from a Git repository using Git LFS pointer
files. The app is a single binary composed of several modules under
`src/` (see "Key files" below). Keep changes minimal and focused: preserve
the program's single-responsibility modules and prefer adding helpers next
to the module being modified.

### Big-picture architecture
- CLI / TUI entrypoint: `src/main.rs` — loads configuration, initialises
  logging and starts the tokio runtime and the `ui` TUI.
- UI layer: `src/ui.rs` — ratatui/crossterm-based TUI. The menu-driven
  actions call into library modules via `tokio::task::spawn_blocking` and
  communicate progress via channels (unbounded tokio mpsc for in‑UI logs).
- Core library: `src/lib.rs` re-exports modules:
  `arma`, `config`, `gitutils`, `modpack`, `downloader` (private module),
  `logging`.
- Filesystem / sync logic: `src/modpack.rs` — parses LFS pointer files,
  computes SHA‑256, copies non-pointer files and (stub) downloads LFS
  objects. Validation logic lives here too.
- Git helpers: `src/gitutils.rs` — clone/open, fetch, head OID lookup. Uses
  `git2` crate and a credentials callback (see env var priority below).
- Downloader worker: `src/downloader.rs` — threaded, producer/consumer
  model with `ProgressEvent`/`ControlCommand` types. `start_download_job`
  returns (progress_rx, control_tx, join_handle).

### Key files to inspect for edits or features
- `src/config.rs` — config lifecycle: location (next to executable),
  `config.txt` creation behavior (only created when missing), `state.txt`
  for private state and repo cache invalidation logic.
- `src/modpack.rs` — pointer parsing (`parse_lfs_pointer_file`),
  `sync_modpack` and `validate_modpack`. Implement provider-specific LFS
  logic here (or call into a new module).
- `src/gitutils.rs` — credential selection: `AZURE_DEVOPS_PAT` first,
  then `GIT_USERNAME`/`GIT_PASSWORD`, then SSH agent. Clone/fetch behavior
  intentionally avoids checkout/merge.
- `src/downloader.rs` — client-side download orchestration and cancellation
  semantics. The UI wires this via `attach_downloader_consumer` in `ui.rs`.
- `README.md` and `Cargo.toml` — useful for build/test expectations and
  dependency versions.

### Developer workflows (build / test / run)
- Build (dev/release): `cargo build` / `cargo build --release`.
- Run (debug): `cargo run` (TUI runs in your terminal). The app will
  create `config.txt` next to the executable on first run.
- Tests: `cargo test` (unit test in `modpack.rs` starts a tiny_http server
  and uses `tests/fixtures/test_blob.bin`). Tests are deterministic and
  use `tempfile` and `tiny_http` (dev-deps).
- VS Code tasks (present in this workspace): `copy: dev config`,
  `build: debug with config` — `copy: dev config` copies
  `config.dev.txt` into `target/debug/config.txt` (helpful for local runs).

### Important runtime/environment details (do not hardcode)
- Configuration path: `config.txt` is stored next to the executable
  (see `Config::config_path`). `Config::load()` creates a default only if
  missing — avoid overwriting an existing file.
- Repo cache path: `repo` directory next to the executable (see
  `Config::repo_cache_path`). `ensure_repo_cache_for_url()` will remove the
  cache when `repo_url` changes and record the previous URL in `state.txt`.
- Logging: file-backed logger writes `modsync.log` next to the executable.
  The logger is not initialised during the Rust test harness unless
  `MODSYNC_FORCE_LOG` is set.

### Environment variables used by the codebase
- AZURE_DEVOPS_PAT — preferred for Azure DevOps git & LFS operations.
  Used by `gitutils` (clone/fetch) and `modpack::download_lfs_object`.
- GIT_USERNAME / GIT_PASSWORD — fallback HTTP credentials for `git2`.
- ARMA3_PATH — `src/arma.rs` will inspect this (and known install
  locations) when auto-detecting the Arma executable.
- MODSYNC_FORCE_LOG — forces file logger init even during tests.

### Project-specific conventions and patterns for contributors
- Config is intentionally user-editable and only created when missing;
  never programmatically overwrite `config.txt` during normal exit.
- Long-running/blocking operations must not block the async UI thread.
  Use `tokio::task::spawn_blocking` (as `ui.rs` demonstrates) or the
  threaded downloader pattern.
- When adding UI menu items, follow `ui.rs`'s pattern: append a string to
  `menu`, handle it in `execute_menu`, clone `self.config` and `log_tx`,
  run blocking work in `spawn_blocking` and push human-readable messages via
  the provided `log_tx` channel.
- Tests may simulate network endpoints (see `modpack` unit test). When
  adding network tests prefer `tiny_http` or other local HTTP servers to
  avoid external calls.

### Integration points and extension notes
- LFS download provider: `modpack::download_lfs_object` currently has
  Azure-style batch API logic and a placeholder fallback (writes an empty
  file). To support other providers (GitHub/GitLab), implement provider
  specific branches here or extract a trait and add implementations.
- Git credentials: `gitutils::build_remote_callbacks` controls credential
  selection. If you add new credential sources (e.g., credential helpers),
  update this function and ensure tests still run without secrets.
- Downloader API: `start_download_job` returns a progress receiver and a
  control sender. Use `attach_downloader_consumer` (in `ui.rs`) as an
  example consumer wiring that converts progress events into UI log
  messages and returns the control sender.

If anything in this summary is unclear or you want more detail on a
specific area (UI patterns, downloader internals, LFS behaviour or the
test harness), tell me which section to expand and I'll iterate.
