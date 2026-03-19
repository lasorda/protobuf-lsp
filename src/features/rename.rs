use crate::workspace::WorkspaceManager;
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Prepare rename: validate that the symbol at cursor is renamable and return its range.
pub fn prepare_rename(
    params: TextDocumentPositionParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<PrepareRenameResponse> {
    let uri = &params.text_document.uri;
    let position = params.position;

    let _proto = workspace.get_file(uri)?;
    let content = content?;

    let line_str = content.lines().nth(position.line as usize)?;
    let symbol = get_word(line_str, position.character as usize);
    if symbol.is_empty() {
        return None;
    }

    // Check that this symbol refers to a renamable entity (message, enum, service, field, method)
    if !is_renamable_symbol(&symbol, workspace, uri) {
        return None;
    }

    let start_col = find_word_start(line_str, position.character as usize);
    let range = Range {
        start: Position {
            line: position.line,
            character: start_col as u32,
        },
        end: Position {
            line: position.line,
            character: start_col as u32 + symbol.len() as u32,
        },
    };

    Some(PrepareRenameResponse::Range(range))
}

/// Rename a symbol across the workspace, producing a WorkspaceEdit.
pub async fn rename(
    params: RenameParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    let _proto = workspace.get_file(&uri)?;
    let content = content?;

    let line_str = content.lines().nth(position.line as usize)?;
    let old_name = get_word(line_str, position.character as usize);
    if old_name.is_empty() {
        return None;
    }

    tracing::debug!("Rename: '{}' -> '{}'", old_name, new_name);

    // Use references logic to find all occurrences
    let ref_params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        context: ReferenceContext {
            include_declaration: true,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let references =
        crate::features::references::find_references(ref_params, workspace, Some(content)).await?;

    if references.is_empty() {
        return None;
    }

    // Group edits by document URI
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for location in references {
        let edit = TextEdit {
            range: location.range,
            new_text: new_name.clone(),
        };
        changes
            .entry(location.uri)
            .or_insert_with(Vec::new)
            .push(edit);
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_word(line: &str, idx: usize) -> String {
    if line.is_empty() {
        return String::new();
    }
    let bytes = line.as_bytes();
    let idx = idx.min(bytes.len().saturating_sub(1));

    let is_word_char = |ch: u8| -> bool { ch.is_ascii_alphanumeric() || ch == b'_' };

    let mut l = idx;
    while l > 0 && is_word_char(bytes[l]) {
        l -= 1;
    }
    if l != idx && !is_word_char(bytes[l]) {
        l += 1;
    }

    let mut r = idx;
    while r < bytes.len() && is_word_char(bytes[r]) {
        r += 1;
    }

    line[l..r].to_string()
}

fn find_word_start(line: &str, idx: usize) -> usize {
    if line.is_empty() {
        return 0;
    }
    let bytes = line.as_bytes();
    let idx = idx.min(bytes.len().saturating_sub(1));

    let is_word_char = |ch: u8| -> bool { ch.is_ascii_alphanumeric() || ch == b'_' };

    let mut l = idx;
    while l > 0 && is_word_char(bytes[l]) {
        l -= 1;
    }
    if l != idx && !is_word_char(bytes[l]) {
        l += 1;
    }
    l
}

/// Check if the symbol is a known renamable entity in the workspace.
fn is_renamable_symbol(symbol: &str, workspace: &WorkspaceManager, uri: &Url) -> bool {
    if let Some(proto) = workspace.get_file(uri) {
        // Check messages
        if proto.find_message_by_name(symbol).is_some() {
            return true;
        }
        // Check enums
        if proto.find_enum_by_name(symbol).is_some() {
            return true;
        }
        // Check services
        if proto.find_service_by_name(symbol).is_some() {
            return true;
        }
        // Check methods
        if proto.find_method_by_name(symbol).is_some() {
            return true;
        }
        // Check fields in messages
        for msg in &proto.messages {
            if msg.fields.iter().any(|f| f.name == symbol) {
                return true;
            }
        }
        // Check enum values
        for e in &proto.enums {
            if e.values.iter().any(|v| v.name == symbol) {
                return true;
            }
        }
    }

    // Also check across workspace
    let results = workspace.find_symbol(symbol);
    !results.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_word() {
        assert_eq!(get_word("message UserRequest {", 8), "UserRequest");
        assert_eq!(get_word("  string name = 1;", 9), "name");
        assert_eq!(get_word("", 0), "");
    }

    #[test]
    fn test_find_word_start() {
        assert_eq!(find_word_start("message UserRequest {", 8), 8);
        assert_eq!(find_word_start("message UserRequest {", 12), 8);
    }
}
