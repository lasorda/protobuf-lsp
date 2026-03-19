# Protobuf Language Server

A high-performance Protocol Buffers language server implemented in Rust, providing complete LSP support for `.proto` files (proto2, proto3, editions).

## Features

### Core LSP Features
- **Code Completion** — Keywords, built-in types, messages, enums, services, and cross-package symbols
- **Go to Definition** — Jump to message/enum/service definitions across files with smart import resolution
- **Find References** — Search all references to a symbol across the current file and all imported files
- **Hover Information** — Display formatted definitions for messages, enums, and services
- **Document Symbols** — Hierarchical outline of packages, imports, messages, enums, and services
- **Code Formatting** — Integrated clang-format support with `.clang-format` file discovery
- **Diagnostics** — Real-time parse errors, duplicate name/field number detection, missing syntax warnings

### Highlights
- **Powered by [proto-rs](https://github.com/lasorda/proto-rs)** — A complete recursive-descent protobuf parser (Rust port of [emicklei/proto](https://github.com/emicklei/proto)) with accurate line/column positions for every AST node
- **Smart Import Resolution** — Searches additional proto directories (highest priority), relative paths, then walks up parent directories
- **Async File Loading** — Dynamically loads uncached import files on demand
- **Cross-package Type Resolution** — Supports package-qualified type names (e.g., `package.MessageName`)

## Installation

### Build from Source
```bash
git clone https://github.com/lasorda/protobuf-lsp.git
cd protobuf-lsp
cargo build --release
```

The executable will be at `target/release/protobuf-lsp`.

## Editor Configuration

### VS Code
Add to your `settings.json`:
```json
{
    "protobuf-langserver.executable": {
        "command": "/path/to/protobuf-lsp/target/release/protobuf-lsp",
        "args": []
    },
    "protobuf-langserver.additionalProtoDirs": [
        "/path/to/shared/proto/files"
    ]
}
```

### Neovim (nvim-lspconfig)
```lua
require'lspconfig'.protobuf_lsp.setup{
    cmd = { "/path/to/protobuf-lsp/target/release/protobuf-lsp" },
    settings = {
        additionalProtoDirs = {
            "/path/to/shared/proto/files",
        }
    }
}
```

### Helix Editor
Add to `~/.config/helix/languages.toml`:
```toml
[[language]]
name = "proto"
language-servers = ["protobuf-lsp"]

[language-server.protobuf-lsp]
command = "/path/to/protobuf-lsp/target/release/protobuf-lsp"

[language-server.protobuf-lsp.settings]
additionalProtoDirs = ["/path/to/shared/proto/files"]
```

## Usage

### Code Completion
Type `.` after a package name to see symbols from that package. General completion includes:
- Protobuf keywords (`syntax`, `message`, `enum`, `service`, `rpc`, `import`, …)
- Scalar types (`string`, `int32`, `int64`, `bool`, `double`, `bytes`, …)
- Messages, enums, and services defined in the current file and imports

### Go to Definition
Place cursor on a type name and press `F12` / `Ctrl+Click`. Works for:
- Message, enum, and service names (including cross-file)
- Import paths (jumps to the imported file)
- Package-qualified names (e.g., `other_package.SomeMessage`)

### Find References
Place cursor on a symbol name and use "Find All References". Searches the current file and all recursively imported files for whole-word matches.

### Import Resolution
The server resolves imports in this order:
1. **Additional proto directories** (configured via `additionalProtoDirs`, highest priority)
2. Relative to the current file's directory
3. Walking up parent directories toward the filesystem root

### Code Formatting
Create a `.clang-format` file in your project (the server searches upward from the proto file):
```yaml
---
Language: Proto
BasedOnStyle: Google
ColumnLimit: 100
IndentWidth: 2
```

### Diagnostics
Real-time checking for:
- Parse errors (with accurate line/column from proto-rs)
- Duplicate message / enum / service names
- Duplicate field numbers within a message
- Missing `syntax` declaration

## Project Structure

```
src/
├── main.rs              # Entry point
├── server.rs            # LSP server (tower-lsp LanguageServer impl)
├── parser/
│   ├── proto.rs         # proto-rs AST → ParsedProto conversion
│   └── resolver.rs      # Import path resolution
├── features/
│   ├── completion.rs    # Code completion
│   ├── definition.rs    # Go to definition
│   ├── references.rs    # Find references
│   ├── hover.rs         # Hover information
│   ├── symbols.rs       # Document symbols
│   ├── formatting.rs    # Code formatting (clang-format)
│   └── diagnostics.rs   # Error diagnostics
└── workspace/
    └── manager.rs       # File cache & import management
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| [proto-parser](https://github.com/lasorda/proto-rs) | Protobuf parser (zero external deps) |
| tower-lsp | LSP framework |
| tokio | Async runtime |
| dashmap | Concurrent HashMap |
| parking_lot | High-performance locks |
| tracing | Structured logging |
| anyhow | Error handling |

## License

MIT License
