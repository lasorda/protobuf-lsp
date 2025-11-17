# Protobuf Language Server

A high-performance Protocol Buffers language server implemented in Rust with complete LSP support.

This project is developed and maintained by CodeBuddy Code, an enterprise-grade AI programming assistant. Most of the code is automatically generated and maintained by CodeBuddy Code.

For more information about CodeBuddy Code, visit: https://copilot.tencent.com/cli/

## Features

### 🚀 Core Features
- **Code Completion** - Support for protobuf keywords, built-in types, messages, enums, and services
- **Go to Definition** - Cross-file navigation with intelligent import resolution
- **Hover Information** - Display detailed information for messages, enums, and services
- **Document Symbols** - Hierarchical display of file structure with support for nested types
- **Code Formatting** - Integrated clang-format support with custom configuration
- **Diagnostics** - Real-time syntax and semantic checking with automatic error refresh

### 🎯 Special Features
- **Smart Import Resolution** - Intelligent import resolution with additional directory support
- **Additional Proto Directories** - Configure additional directories to search for proto files with highest priority
- **Async File Loading** - Dynamic loading of uncached import files
- **Cross-package Type Resolution** - Support for package-qualified type names (e.g., `package.Type`)
- **RPC Method Support** - Complete parsing of service and method definitions

## Installation

### Build from Source
```bash
git clone https://github.com/lasorda/protobuf-lsp.git
cd protobuf-lsp
cargo build --release
```

After running `cargo build --release`, the executable will be located at `target/release/protobuf-lsp`.

## Configuration

### VS Code
Add to your `settings.json`:
```json
{
    "protobuf-langserver.trace.server": "messages",
    "protobuf-langserver.executable": {
        "command": "/path/to/protobuf-lsp/target/release/protobuf-lsp",
        "args": []
    }
}
```

### Neovim
Using nvim-lspconfig:
```lua
require'lspconfig'.protobuf_lsp.setup{
    cmd = {"/path/to/protobuf-lsp/target/release/protobuf-lsp"},
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
```

## Usage

### Code Completion
Supports completion for:
- Protobuf keywords (`syntax`, `message`, `enum`, `service`, `rpc`, `import`, `package`)
- Scalar types (`string`, `int32`, `int64`, `bool`, `double`, `bytes`, etc.)
- Defined messages, enums, and services
- Types from imported files
- Package-qualified types (type `package.` to see all types in that package)

### Go to Definition
- Place cursor on a type name and use `F12` or `Ctrl+Click` to jump
- Supports cross-file navigation
- Automatically resolves import paths with upward search

### Import Resolution
The language server intelligently searches for imported files:
1. **Additional proto directories** (highest priority, if configured)
2. Relative to the current file's directory
3. Walking up parent directories

#### Configuring Additional Proto Directories

You can configure additional directories to search for proto files. These directories have the highest priority when resolving imports.

**VS Code configuration:**
```json
{
    "protobuf-langserver.trace.server": "messages",
    "protobuf-langserver.executable": {
        "command": "/path/to/protobuf-lsp/target/release/protobuf-lsp",
        "args": []
    },
    "protobuf-langserver.additionalProtoDirs": [
        "/path/to/shared/proto/files",
        "/path/to/external/api/proto"
    ]
}
```

**Neovim configuration:**
```lua
require'lspconfig'.protobuf_lsp.setup{
    cmd = {"/path/to/protobuf-lsp/target/release/protobuf-lsp"},
    settings = {
        additionalProtoDirs = {
            "/path/to/shared/proto/files",
            "/path/to/external/api/proto"
        }
    }
}
```

**Helix Editor configuration:**
```toml
[language-server.protobuf-lsp]
command = "/path/to/protobuf-lsp/target/release/protobuf-lsp"

[language-server.protobuf-lsp.settings]
additionalProtoDirs = [
    "/path/to/shared/proto/files",
    "/path/to/external/api/proto"
]
```

Example directory structure:
```
project/
├── common/
│   └── types.proto
└── api/
    └── service.proto

external-protos/
└── google/
    └── protobuf/
        └── empty.proto
```

In `service.proto`:
```protobuf
import "common/types.proto";        // Found from relative path
import "google/protobuf/empty.proto"; // Found from additional-proto-dirs
```

### Code Formatting
Supports formatting with clang-format:
1. Create a `.clang-format` file in your project root or any parent directory
2. The language server will automatically find and apply the configuration

Example `.clang-format`:
```yaml
---
Language: Proto
BasedOnStyle: Google
ColumnLimit: 100
IndentWidth: 2
UseTab: Never
```

### Diagnostics
Real-time checking for:
- Syntax errors
- Duplicate message/enum/service names
- Duplicate field numbers
- Missing syntax declaration

Errors automatically refresh after file modifications.

## Project Structure

```
src/
├── main.rs          # Program entry point
├── server.rs        # LSP server implementation
├── parser/          # Protobuf parser
│   ├── mod.rs       # Module exports
│   └── proto.rs     # Parser implementation
├── features/        # LSP feature implementations
│   ├── completion.rs    # Code completion
│   ├── definition.rs    # Go to definition
│   ├── hover.rs         # Hover information
│   ├── symbols.rs       # Document symbols
│   ├── formatting.rs    # Code formatting
│   └── diagnostics.rs   # Error diagnostics
└── workspace/       # Workspace management
    ├── mod.rs
    └── manager.rs   # File cache management
```

## Technology Stack

- **tower-lsp** - Mature LSP framework
- **tokio** - Async runtime
- **protobuf-parse** - Pure Rust protobuf parser
- **dashmap** - Lock-free concurrent HashMap
- **parking_lot** - High-performance locks
- **tracing** - Structured logging

## Performance Advantages

Compared to the Go version:
- **Zero-cost abstractions** - Rust's zero-overhead features
- **Memory safety** - No data races, no GC needed
- **Concurrency safety** - Compile-time concurrency checks
- **Type safety** - Stronger type system guarantees

## Troubleshooting

### clang-format not found
Ensure clang-format is installed:
```bash
# Ubuntu/Debian
sudo apt install clang-format

# macOS
brew install clang-format

# Windows
# Download LLVM and add to PATH
```

### Import files not resolving
1. Check if file paths are correct
2. Ensure files exist
3. The language server automatically searches upward, no additional path configuration needed

## Contributing

Issues and Pull Requests are welcome!

## License

MIT License