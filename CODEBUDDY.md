# CODEBUDDY.md

This file provides guidance to CodeBuddy Code when working with code in this repository.

## Project Overview

`protobuf-lsp` is a Language Server Protocol implementation for Protocol Buffers (`.proto` files, proto2/proto3/editions), written in Rust. It communicates with editors (VS Code, Neovim, Helix, Zed) over stdio via `tower-lsp`, and is published to crates.io as `protobuf-lsp`.

Parsing is delegated to the external [`proto-parser`](https://github.com/lasorda/proto-rs) crate (a Rust port of `emicklei/proto`), which provides accurate line/column positions for every AST node. This LSP layer converts that AST into its own `ParsedProto` representation and exposes ~15 LSP features on top of it.

## Common Commands

```bash
# Build (debug)
cargo build

# Build (release) — produces target/release/protobuf-lsp
cargo build --release

# Run the server (reads LSP JSON-RPC from stdin, writes to stdout)
cargo run --release
# or: ./target/release/protobuf-lsp

# Run all tests
cargo test

# Run a single test (by name substring)
cargo test test_duplicate_message_names
cargo test --test completion_test test_completion_includes_existing_messages_after_incomplete_edit

# Run tests in a specific module with verbose output
cargo test --verbose parser::resolver::tests

# Run the line-number fixture binary (manual smoke test against a real proto file)
cargo run --example test_complex_options path/to/file.proto

# Publish (automated via .github/workflows/publish.yml on push to master)
cargo publish
```

Formatting/linting: there is no rustfmt/clippy config checked in beyond Cargo defaults. CI (`.github/workflows/rust.yml`) only runs `cargo build --verbose` and `cargo test --verbose`.

## Architecture

The code is organized into four layers, each in its own module under `src/`. Data flows top-down: the LSP server receives requests → workspace manager serves cached parse results → feature modules compute responses. Parsing happens lazily on `did_open` / `did_change`.

### `src/server.rs` — LSP entry point
Implements `tower_lsp::LanguageServer` for `ProtobufLanguageServer`. The struct holds:
- `client: Client` — for sending diagnostics/log messages back to the editor
- `workspace: Arc<WorkspaceManager>` — shared parse cache + import resolver
- `document_contents: Arc<DashMap<Url, String>>` — live text of open files (needed because features like completion receive only a position, not the text)

Every LSP handler is a thin shim: it looks up the document content from `document_contents`, then delegates to a `features::*` function. Server capabilities are declared in `initialize` (note: `TextDocumentSyncKind::FULL` — the whole document is sent on every change, not incremental edits).

`additionalProtoDirs` (import search paths) can be configured via `initialization_options` at startup or via `did_change_configuration` at runtime; both feed into `WorkspaceManager::add_proto_directory`.

### `src/workspace/manager.rs` — `WorkspaceManager`
Thread-safe cache of parsed proto files, built on `DashMap` and `parking_lot::RwLock`. This is the shared state every feature reads from.

**Last-good cache (important invariant):** The manager keeps three maps per URI:
- `files` — live cache of the most recent *successful* parse (what features read)
- `last_good` — same as `files` on success; left untouched on parse failure (used to restore `files` if needed)
- `last_errors` — parse errors from the most recent attempt (success clears this)

`open_file` parses the new content; on **success** it updates `files` + `last_good` and clears `last_errors`; on **failure** it leaves `files`/`last_good` unchanged, records the error in `last_errors`, and returns the `last_good` result (or `Err` if the file was never successfully parsed). This lets completion/definition/hover keep working while the user is mid-edit on a syntactically broken file — see `tests/completion_test.rs` for the contract.

Import resolution is delegated to `ImportResolver` (in `parser/resolver.rs`). `get_imported_file` is async and will load uncached files from disk on demand; `get_imported_file_cached` is sync and cache-only. `collect_all_imports_recursive_async` walks transitive imports with a `visited` set to handle circular imports.

### `src/parser/` — parsing layer
- `proto.rs` — `ProtoParser` wraps `proto_parser::Parser` and converts its AST into the `ParsedProto` / `MessageElement` / `EnumElement` / `ServiceElement` / `FieldElement` / `MethodElement` / `ExtendElement` structs used throughout the LSP. `parse()` returns `Result<ParsedProto>`: `Ok` on success (cached), `Err(ParseError)` on failure (not cached). `ParseError` carries 0-indexed line/column + severity. `ProtoElement` is an enum used in `line_to_element` for quick position→element lookup. All line numbers in the converted structures are 0-indexed (the `pos_line`/`pos_col` helpers adjust from the parser's 1-indexed positions).
- `resolver.rs` — `ImportResolver::resolve_import` search order: (1) `additional_dirs` (highest priority), (2) relative to the importing file's directory, (3) walk up parent directories toward the filesystem root. Returns `Option<PathBuf>` — `None` means the import could not be resolved.

### `src/features/` — LSP feature implementations
Each file exports a free function (typically `provide_*` / `find_*` / `validate_*`) that takes `WorkspaceManager` + params + optional document content and returns the LSP response type. `mod.rs` re-exports them. Current features:

| File | Function | LSP method |
|------|----------|------------|
| `completion.rs` | `provide_completion` | `textDocument/completion` (triggers: `.`, `:`) |
| `definition.rs` | `provide_definition_async` | `textDocument/definition` |
| `references.rs` | `find_references` | `textDocument/references` |
| `rename.rs` | `prepare_rename`, `rename` | `textDocument/prepareRename`, `textDocument/rename` |
| `hover.rs` | `provide_hover_async` | `textDocument/hover` |
| `symbols.rs` | `provide_document_symbols` | `textDocument/documentSymbol` |
| `workspace_symbols.rs` | `workspace_symbol` | `workspace/symbol` |
| `signature_help.rs` | `provide_signature_help` | `textDocument/signatureHelp` (trigger: `(`) |
| `code_actions.rs` | `provide_code_actions` | `textDocument/codeAction` (quickfixes + sort imports) |
| `semantic_tokens.rs` | `provide_semantic_tokens_full` | `textDocument/semanticTokens/full` |
| `folding.rs` | `provide_folding_ranges` | `textDocument/foldingRange` |
| `document_link.rs` | `provide_document_links` | `textDocument/documentLink` |
| `formatting.rs` | `format_document`, `format_range` | `textDocument/formatting`, `textDocument/rangeFormatting` |
| `diagnostics.rs` | `validate_proto_file`, `publish_diagnostics` | (pushed on `did_open`/`did_change`) |

**Diagnostics flow:** `did_open`/`did_change` call `workspace.open_file`, then `validate_proto_file` regardless of parse success. `validate_proto_file` reads the cached `ParsedProto` for *semantic* checks (duplicate names/field numbers, missing syntax) and `workspace.get_last_errors` for *syntax* errors from the most recent parse attempt. An empty diagnostic list is published as `[]` to clear previous errors (LSP semantics).

**Formatting flow:** `formatting.rs` shells out to the `clang-format` binary. It searches upward from the proto file's directory for a `.clang-format` file; if none is found, formatting is a no-op (returns `None`). The clang-format binary is located via `which clang-format`, then a hardcoded fallback list — note one entry is a machine-specific path (`/home/zhihaopan/.local/llvm20/...`) that won't exist elsewhere.

## Tests

- **Unit tests** (`#[cfg(test)] mod tests`): inside `parser/proto.rs`, `parser/resolver.rs`, `workspace/manager.rs`, `features/diagnostics.rs`. Cover import resolution priority, workspace cache lifecycle, duplicate detection. Use `tempfile` for filesystem fixtures.
- **Integration tests** (`tests/completion_test.rs`): async tests (`#[tokio::test]`) that build a `WorkspaceManager`, drive it through `open_file` with valid then invalid content, and assert completion/`get_last_errors` behavior. This is the contract test for the last-good cache — read it before changing `WorkspaceManager::open_file`.
- **Fixture binary** (`tests/line_number_fix/test_line_numbers.rs`, run via `examples/test_complex_options.rs`): a manual smoke test that parses a real `teams.proto` and checks message line numbers. Not part of `cargo test` proper; run via the `run_test.sh` script or `cargo run --example`.

## Configuration surface

LSP clients configure the server via:
- `initialization_options.additionalProtoDirs: string[]` — extra directories searched first for `import` resolution.
- `settings.additionalProtoDirs` (sent via `workspace/didChangeConfiguration`) — same, applied at runtime.

There is no other settings surface; all other behavior is hardcoded (e.g. completion trigger characters, sync kind, formatting style).

## Conventions

- **Line/column indexing:** All positions in `ParsedProto` and its elements are **0-indexed**, matching LSP `Position`. The underlying `proto-parser` is 1-indexed; conversion happens in `convert_proto` via `pos_line`/`pos_col`.
- **Error handling:** `anyhow::Result` for fallible operations; `ParseError` (with line/column) for parse failures specifically. Feature functions return `Option<LspType>` and silently return `None` when the file isn't cached or content is unavailable — they do not propagate errors to the server.
- **Concurrency:** `WorkspaceManager` is `Clone` (all fields are `Arc`). Features receive `&WorkspaceManager`. `DashMap` for per-file caches, `parking_lot::RwLock` for the `ImportResolver` (read-heavy). No `tokio::Mutex` is used for cache state.
- **Logging:** `tracing` macros (`tracing::info!` / `debug!` / `warn!` / `error!`). Initialized in `main.rs` with `EnvFilter` defaulting to `protobuf_lsp=info`, output to stderr (so it doesn't corrupt the stdio LSP protocol). Use `RUST_LOG=protobuf_lsp=debug` for verbose import-resolution / completion logs.

## Release workflow

Releases are cut from `master` after a fix/feature has landed. Steps:

1. **Bump version** in both `Cargo.toml` (`[package] version`) and `Cargo.lock` (the `protobuf-lsp` package entry). The two must stay in sync — `cargo build` does not auto-update the lock for the workspace crate's own version field.
2. **Commit** the version bump together with the code changes (or as a follow-up commit on `master`). Conventional commit style: `fix(hover): ...`, `feat(completion): ...`, etc.
3. **Create an annotated tag** `vX.Y.Z` with a short message summarizing the change:
   ```bash
   git tag -a v0.1.6 -m "v0.1.6: fix hover on symbols from imported proto files not opened in editor"
   ```
4. **Push** the branch and the tag:
   ```bash
   git push origin master
   git push origin vX.Y.Z
   ```
5. **Publishing to crates.io** is automated via `.github/workflows/publish.yml` triggered by the tag push — do NOT run `cargo publish` manually.

Versioning follows SemVer:
- **patch** (0.1.X): bug fixes, no behavior contract changes (e.g. hover now works on imported-but-unopened files).
- **minor** (0.X.0): new LSP features or user-visible config additions.
- **major** (X.0.0): breaking changes to the LSP API or config surface (none expected pre-1.0).

### Sync vs async feature functions

When adding or modifying an LSP feature that needs to resolve symbols across imports, prefer the **async** variant (`provide_*_async`) and use `WorkspaceManager::collect_all_imports_async` to load transitive imports on demand. The sync variants (`provide_*` + `get_imported_file_cached`) only see files that the editor has `did_open`-ed and will silently miss symbols from unopened imports — keep them only as `#[allow(dead_code)]` fallbacks or for tests that assert the sync limitation. See `definition.rs` (`provide_definition_async`) and `hover.rs` (`provide_hover_async`) as reference patterns.

