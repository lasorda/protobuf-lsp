use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::{Location, Position, Range, ReferenceParams, Url};

/// Find all references to the symbol at the given cursor position.
///
/// The algorithm mirrors the Go implementation in
/// `protobuf-language-server/components/references.go`:
///
/// 1. Extract the symbol name (without package prefix) under the cursor.
/// 2. Optionally include the declaration site itself.
/// 3. Search the current file for whole-word occurrences (skipping comments,
///    imports and blank lines).
/// 4. Recursively search all imported files, using a visited-set to avoid
///    cycles.
pub async fn find_references(
    params: ReferenceParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<Vec<Location>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;

    let proto = workspace.get_file(&uri)?;
    let content = content?;

    // Extract the word at cursor (without dots – we want the simple symbol name)
    let line_str = content.lines().nth(position.line as usize).unwrap_or("");
    let symbol_name = get_word(line_str, position.character as usize, false);
    if symbol_name.is_empty() {
        return Some(Vec::new());
    }

    tracing::debug!("FindReferences: looking for symbol '{}'", symbol_name);

    let mut results: Vec<Location> = Vec::new();

    // Track definition location so we can skip it in file search to avoid duplicates
    let mut def_uri: Option<Url> = None;
    let mut def_line: Option<u32> = None;

    // If includeDeclaration, find the definition and add it
    if params.context.include_declaration {
        // Quick search: look in current file for message/enum/service with this name
        if let Some(msg) = proto.find_message_by_name(&symbol_name) {
            def_uri = Some(uri.clone());
            def_line = Some(msg.line);
            results.push(make_location(&uri, msg.line, msg.character, symbol_name.len()));
        } else if let Some(e) = proto.find_enum_by_name(&symbol_name) {
            def_uri = Some(uri.clone());
            def_line = Some(e.line);
            results.push(make_location(&uri, e.line, e.character, symbol_name.len()));
        } else if let Some(svc) = proto.find_service_by_name(&symbol_name) {
            def_uri = Some(uri.clone());
            def_line = Some(svc.line);
            results.push(make_location(&uri, svc.line, svc.character, symbol_name.len()));
        }
    }

    // Search current file
    search_file_for_references(
        content,
        &uri,
        &symbol_name,
        def_uri.as_ref(),
        def_line,
        &mut results,
    );

    // Recursively search imported files
    let mut searched: std::collections::HashSet<String> = std::collections::HashSet::new();
    searched.insert(uri.to_string());
    search_imported_files(
        workspace,
        &proto,
        &uri,
        &symbol_name,
        &mut searched,
        &mut results,
        def_uri.as_ref(),
        def_line,
    )
    .await;

    tracing::debug!("FindReferences: found {} references", results.len());
    Some(results)
}

/// Recursively search imported files for references.
async fn search_imported_files(
    workspace: &WorkspaceManager,
    proto: &crate::parser::ParsedProto,
    current_uri: &Url,
    symbol_name: &str,
    searched: &mut std::collections::HashSet<String>,
    results: &mut Vec<Location>,
    def_uri: Option<&Url>,
    def_line: Option<u32>,
) {
    for imp in &proto.imports {
        // Try to resolve and load the imported file
        let imported = match workspace.get_imported_file(current_uri, &imp.path).await {
            Some(f) => f,
            None => {
                if let Some(f) = workspace.get_imported_file_cached(current_uri, &imp.path) {
                    f
                } else {
                    continue;
                }
            }
        };

        let import_uri_str = imported.uri.clone();
        if searched.contains(&import_uri_str) {
            continue;
        }
        searched.insert(import_uri_str.clone());

        let import_url = match Url::parse(&import_uri_str) {
            Ok(u) => u,
            Err(_) => continue,
        };

        // Read imported file content via filesystem
        let file_content = match read_file_from_uri(&import_url) {
            Some(c) => c,
            None => continue,
        };

        search_file_for_references(
            &file_content,
            &import_url,
            symbol_name,
            def_uri,
            def_line,
            results,
        );

        // Recurse into this file's imports
        Box::pin(search_imported_files(
            workspace,
            &imported,
            &import_url,
            symbol_name,
            searched,
            results,
            def_uri,
            def_line,
        ))
        .await;
    }
}

/// Search a single file's content for all whole-word occurrences of `symbol_name`.
fn search_file_for_references(
    content: &str,
    file_uri: &Url,
    symbol_name: &str,
    def_uri: Option<&Url>,
    def_line: Option<u32>,
    results: &mut Vec<Location>,
) {
    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num as u32;

        // Skip definition line to avoid duplicates
        if let (Some(du), Some(dl)) = (def_uri, def_line) {
            if du == file_uri && dl == line_num {
                continue;
            }
        }

        let trimmed = line.trim();

        // Skip empty, import, and comment lines
        if trimmed.is_empty()
            || trimmed.starts_with("import")
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
        {
            continue;
        }

        // Find all occurrences
        let mut idx = 0usize;
        while let Some(found) = line[idx..].find(symbol_name) {
            let abs_pos = idx + found;
            if is_whole_word(line, abs_pos, symbol_name.len()) {
                results.push(make_location(file_uri, line_num, abs_pos as u32, symbol_name.len()));
            }
            idx = abs_pos + 1;
            if idx >= line.len() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check whether the match at `start` for `length` bytes is a whole word (not
/// part of a larger identifier).
fn is_whole_word(line: &str, start: usize, length: usize) -> bool {
    let bytes = line.as_bytes();
    // Check character before
    if start > 0 {
        let prev = bytes[start - 1];
        if is_identifier_char(prev) {
            return false;
        }
    }
    // Check character after
    let end = start + length;
    if end < bytes.len() {
        let next = bytes[end];
        if is_identifier_char(next) {
            return false;
        }
    }
    true
}

fn is_identifier_char(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

/// Extract the word at `idx` in `line`, optionally including dots.
fn get_word(line: &str, idx: usize, include_dot: bool) -> String {
    if line.is_empty() {
        return String::new();
    }
    let bytes = line.as_bytes();
    let idx = idx.min(bytes.len().saturating_sub(1));

    let is_word_char = |ch: u8| -> bool {
        ch.is_ascii_alphanumeric() || ch == b'_' || (include_dot && ch == b'.')
    };

    let mut l = idx;
    while l > 0 && is_word_char(bytes[l]) {
        l -= 1;
    }
    if l != idx && !is_word_char(bytes[l]) {
        l += 1;
    }
    // Edge: if l == 0 and is word char, keep it

    let mut r = idx;
    while r < bytes.len() && is_word_char(bytes[r]) {
        r += 1;
    }

    line[l..r].to_string()
}

fn make_location(uri: &Url, line: u32, character: u32, name_len: usize) -> Location {
    Location {
        uri: uri.clone(),
        range: Range {
            start: Position { line, character },
            end: Position {
                line,
                character: character + name_len as u32,
            },
        },
    }
}

/// Read file content from a file:// URI.
fn read_file_from_uri(uri: &Url) -> Option<String> {
    let path = uri.to_file_path().ok()?;
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_word() {
        assert_eq!(get_word("string user_id = 1;", 7, false), "user_id");
        assert_eq!(get_word("string user_id = 1;", 0, false), "string");
        assert_eq!(get_word("pkg.Msg", 4, true), "pkg.Msg");
        assert_eq!(get_word("pkg.Msg", 4, false), "Msg");
        assert_eq!(get_word("", 0, false), "");
    }

    #[test]
    fn test_is_whole_word() {
        assert!(is_whole_word("User user = 1;", 0, 4)); // "User"
        assert!(is_whole_word("User user = 1;", 5, 4)); // "user"
        assert!(!is_whole_word("UserInfo user = 1;", 0, 4)); // "User" inside "UserInfo"
        assert!(!is_whole_word("some_User = 1;", 5, 4)); // "User" inside "some_User"
    }
}
