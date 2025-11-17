/// Protobuf document formatting using clang-format with .clang-format configuration
///
/// This module provides formatting support for .proto files using clang-format.
/// It follows standard project conventions by searching for .clang-format files
/// in the following order:
///
/// 1. Start from the directory containing the .proto file
/// 2. Search upward through parent directories
/// 3. Use the first .clang-format file found
///
/// If no .clang-format file is found, formatting will not be applied.
/// This ensures that project-specific formatting rules are respected.
///
/// The formatter also handles range formatting for partial document formatting.
use tower_lsp::lsp_types::{DocumentFormattingParams, Range, TextEdit};
use std::process::Command;
use std::path::{Path, PathBuf};

pub fn format_document(params: DocumentFormattingParams, content: &str) -> Option<Vec<TextEdit>> {
    // Extract the file path from the URI
    let uri = &params.text_document.uri;
    let file_path = uri.to_file_path().ok()?;

    // Find .clang-format file
    let clang_format_path = find_clang_format_file(&file_path)?;

    // Try to use clang-format with the found configuration file
    match format_with_clang_format(content, &clang_format_path) {
        Ok(formatted) => {
            if formatted != content {
                // Return a single edit that replaces the entire document
                Some(vec![TextEdit {
                    range: Range {
                        start: tower_lsp::lsp_types::Position {
                            line: 0,
                            character: 0,
                        },
                        end: tower_lsp::lsp_types::Position {
                            line: u32::MAX,
                            character: u32::MAX,
                        },
                    },
                    new_text: formatted,
                }])
            } else {
                None
            }
        }
        Err(e) => {
            tracing::warn!("Failed to format with clang-format using {}: {}", clang_format_path.display(), e);
            None
        }
    }
}

/// Find .clang-format file by searching from the file's directory up to the root
/// Follows the standard project rules: uses the first .clang-format found when searching upward
fn find_clang_format_file(file_path: &Path) -> Option<PathBuf> {
    let mut current_dir = file_path.parent()?;

    loop {
        let clang_format_path = current_dir.join(".clang-format");
        if clang_format_path.exists() && clang_format_path.is_file() {
            tracing::debug!("Found .clang-format at: {}", clang_format_path.display());
            return Some(clang_format_path);
        }

        // Move up to parent directory
        match current_dir.parent() {
            Some(parent) => {
                current_dir = parent;
                // Stop at filesystem root
                if current_dir == Path::new("/") {
                    break;
                }
            }
            None => break,
        }
    }

    tracing::debug!("No .clang-format file found for {}", file_path.display());
    None
}

/// Find clang-format binary in common locations
fn find_clang_format_binary() -> Option<String> {
    // Common paths where clang-format might be installed
    let common_paths = vec![
        "/usr/bin/clang-format",
        "/usr/local/bin/clang-format",
        "/opt/homebrew/bin/clang-format",
        "/home/zhihaopan/.local/llvm20/build/bin/clang-format",
    ];

    // First check PATH
    if let Ok(output) = Command::new("which").arg("clang-format").output() {
        if output.status.success() {
            return Some("clang-format".to_string());
        }
    }

    // Then check common paths
    for path in common_paths {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    None
}

fn format_with_clang_format(content: &str, clang_format_path: &Path) -> Result<String, std::io::Error> {
    // Try to find clang-format in common paths
    let clang_format_bin = find_clang_format_binary().unwrap_or_else(|| "clang-format".to_string());

    let mut child = Command::new(clang_format_bin)
        .arg("--assume-filename=file.proto")
        .arg("--style=file")
        .arg("--fallback-style=none") // Don't use fallback style if .clang-format is not found
        .current_dir(clang_format_path.parent().unwrap_or(clang_format_path))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(content.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("clang-format failed: {}", stderr),
        ))
    }
}

pub fn format_range(params: DocumentFormattingParams, content: &str, range: Range) -> Option<Vec<TextEdit>> {
    // Extract the file path from the URI
    let uri = &params.text_document.uri;
    let file_path = uri.to_file_path().ok()?;

    // Find .clang-format file
    let clang_format_path = find_clang_format_file(&file_path)?;

    // For range formatting, we need to calculate line offsets
    let lines: Vec<&str> = content.lines().collect();
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    if start_line >= lines.len() {
        return None;
    }

    // Extract the range content (simplified - includes whole lines)
    let range_content = if end_line >= lines.len() {
        lines[start_line..].join("\n")
    } else {
        lines[start_line..=end_line].join("\n")
    };

    // Format just the range content
    match format_with_clang_format(&range_content, &clang_format_path) {
        Ok(formatted_range) => {
            if formatted_range != range_content {
                Some(vec![TextEdit {
                    range,
                    new_text: formatted_range,
                }])
            } else {
                None
            }
        }
        Err(e) => {
            tracing::warn!("Failed to format range with clang-format: {}", e);
            None
        }
    }
}
