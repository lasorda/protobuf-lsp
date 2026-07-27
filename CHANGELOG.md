# Changelog

All notable changes to this project will be documented in this file.

## [0.1.5] - 2026-07-27

### Fixed
- Completion now works while editing, even when the proto file is temporarily
  unparseable (e.g. a half-typed field). Previously the parser returned an empty
  result on any syntax error, which wiped out all known messages/enums/packages
  and left only keyword completions. The workspace now keeps the last successful
  parse result and serves completion/definition/hover from it while recording the
  parse error separately for diagnostics.

### Changed
- `ProtoParser::parse` now returns `Err(ParseError)` on parse failure instead of
  fabricating an empty `ParsedProto`.
- `WorkspaceManager` maintains a `last_good` cache and a `last_errors` cache per
  file; `get_last_errors` exposes the most recent parse errors for diagnostics.
- `validate_proto_file` reads parse errors from `workspace.get_last_errors`
  instead of `ParsedProto::parse_errors`.
- Removed unused `create_parse_diagnostics` / `extract_line_from_error`.

## [0.1.0] - 2025-11-16

### Added
- Initial implementation of Protobuf Language Server in Rust
- Core LSP features:
  - Code completion for protobuf keywords, types, messages, enums, and services
  - Go to definition with cross-file support
  - Hover information with markdown formatting
  - Document symbols showing package, imports, messages, enums, services, and RPCs
  - Full document and range formatting using clang-format

- Special features:
  - RPC method parsing and display in document symbols
  - Support for proto files with custom options (using fallback parser)
  - .clang-format configuration file support
    - Searches upward from file directory following standard project rules
    - Respects project-specific formatting configurations
    - Skips formatting if no configuration found

- Dynamic Import Resolution:
  - Intelligent upward directory search for import files
  - No hardcoded base directories - works with any project structure
  - Cross-platform compatibility (Windows, macOS, Linux)
  - Search order: relative path → upward search → additional directories

- Enhanced Go-to-Definition:
  - Support for qualified type names (e.g., `package.Type`)
  - Async file loading for uncached imports
  - Improved word extraction including dots for qualified names
  - Package-aware type resolution

- Technical implementation:
  - Modular architecture with separate parser, features, and workspace modules
  - Dual parsing strategy: protobuf-parse with simple text-based fallback
  - Caching system for parsed files
  - Async/await support throughout

### Fixed
- Service parsing when "service" keyword has no leading space
- Fallback logic when protobuf-parse fails on files with custom options
- Parsing of services that appear after messages in the same file
- Go-to-definition for qualified types with package prefixes
- Import resolution without hardcoded paths
- Word extraction at cursor position for qualified names

### Changed
- Replaced hardcoded QQMail directory with dynamic upward search
- Improved import resolver to work with any project structure
- Enhanced word extraction to include dots for qualified names
- Split qualified names into package and simple name for better resolution

### Documentation
- Comprehensive README with setup instructions (updated to English)
- Feature descriptions and usage examples
- Architecture overview and technology choices
- Import resolution examples and configuration guide

### Known Limitations
- Custom options parsing is limited in fallback mode
- Position information may be approximate in simple parser
- clang-format must be installed for formatting functionality