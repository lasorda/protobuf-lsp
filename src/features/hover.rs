use crate::parser::proto::{MessageElement, EnumElement, ServiceElement};
use crate::parser::ParsedProto;
use crate::workspace::WorkspaceManager;
use std::sync::Arc;
use tower_lsp::lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Position, Url};

/// Extract the word at the given position from the content
fn extract_word_at_position(content: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let char_pos = position.character as usize;

    if char_pos > line.len() {
        return None;
    }

    // Find word boundaries
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }

    // Handle cursor at end of line or beyond - try the character before
    let mut check_pos = if char_pos >= chars.len() && char_pos > 0 {
        char_pos - 1
    } else if char_pos >= chars.len() {
        return None;
    } else {
        char_pos
    };

    // Check if current position is on a word character, if not try the previous character
    if !chars[check_pos].is_alphanumeric() && chars[check_pos] != '_' {
        if check_pos > 0 && (chars[check_pos - 1].is_alphanumeric() || chars[check_pos - 1] == '_') {
            check_pos -= 1;
        } else {
            return None;
        }
    }

    // Find start of word
    let mut start = check_pos;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    // Find end of word
    let mut end = check_pos;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    Some(chars[start..end].iter().collect())
}

/// Search for message recursively (including nested messages)
fn find_message_recursive<'a>(
    messages: &'a [MessageElement],
    name: &str,
) -> Option<&'a MessageElement> {
    for msg in messages {
        if msg.name == name {
            return Some(msg);
        }
        // Search in nested messages
        if let Some(nested) = find_message_recursive(&msg.nested_messages, name) {
            return Some(nested);
        }
    }
    None
}

/// Search for enum recursively (including nested enums in messages)
fn find_enum_recursive<'a>(
    messages: &'a [MessageElement],
    enums: &'a [EnumElement],
    name: &str,
) -> Option<&'a EnumElement> {
    // Search in top-level enums
    for e in enums {
        if e.name == name {
            return Some(e);
        }
    }

    // Search in nested enums within messages
    for msg in messages {
        if let Some(e) = find_enum_in_message(msg, name) {
            return Some(e);
        }
    }
    None
}

fn find_enum_in_message<'a>(
    msg: &'a MessageElement,
    name: &str,
) -> Option<&'a EnumElement> {
    for e in &msg.nested_enums {
        if e.name == name {
            return Some(e);
        }
    }
    // Search in nested messages
    for nested_msg in &msg.nested_messages {
        if let Some(e) = find_enum_in_message(nested_msg, name) {
            return Some(e);
        }
    }
    None
}

#[allow(dead_code)]
pub fn provide_hover(params: HoverParams, workspace: &WorkspaceManager, content: Option<&str>) -> Option<Hover> {
    provide_hover_impl(params, workspace, content, &|uri| {
        // Synchronous version: only consults the cache. Imports that have not
        // been opened in the editor will be missing, so hover on references to
        // such symbols returns None. Use `provide_hover_async` for on-demand
        // loading of imported files.
        let imports: Vec<Arc<ParsedProto>> = proto_imports_cached(workspace, uri);
        imports
    })
}

/// Collect direct imports using the synchronous (cache-only) API.
#[allow(dead_code)]
fn proto_imports_cached(workspace: &WorkspaceManager, uri: &Url) -> Vec<Arc<ParsedProto>> {
    let proto = match workspace.get_file(uri) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for import in &proto.imports {
        if let Some(imported) = workspace.get_imported_file_cached(uri, &import.path) {
            out.push(imported);
        }
    }
    out
}

/// Async version of `provide_hover` that can load imported files on demand.
///
/// Unlike the synchronous variant, this will resolve and parse imported proto
/// files that have not yet been opened in the editor, so hover works on
/// references to symbols defined in such imports.
pub async fn provide_hover_async(
    params: HoverParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri.clone();
    // Eagerly collect imports using the async API (loads missing files from disk).
    let imports: Vec<Arc<ParsedProto>> = workspace.collect_all_imports_async(&uri).await;
    provide_hover_impl(params, workspace, content, &|_| imports.clone())
}

fn provide_hover_impl<F>(
    params: HoverParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
    collect_imports: &F,
) -> Option<Hover>
where
    F: Fn(&Url) -> Vec<Arc<ParsedProto>>,
{
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let proto = workspace.get_file(&uri)?;

    // Try to find element at the position (for hover on definition line)
    if let Some(element) = proto.find_element_at_position(position) {
        let content = match element {
            crate::parser::ProtoElement::Message(msg) => format_message_hover(msg),
            crate::parser::ProtoElement::Enum(e) => format_enum_hover(e),
            crate::parser::ProtoElement::Service(svc) => format_service_hover(svc),
            crate::parser::ProtoElement::Field(field) => {
                format!("**Field**: {} {}\n\nField number: {}", field.field_type, field.name, field.number)
            }
            crate::parser::ProtoElement::Method(method) => {
                format!("**Method**: {}\n\nInput: {}\nOutput: {}\nClient streaming: {}\nServer streaming: {}",
                       method.name,
                       method.input_type,
                       method.output_type,
                       method.client_streaming,
                       method.server_streaming)
            }
        };

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        });
    }

    // Extract the word at the cursor position for hover on references
    let symbol_name = if let Some(content) = content {
        extract_word_at_position(content, position)?
    } else {
        return None;
    };

    tracing::debug!("Hover: extracted symbol name '{}' at position {}:{}", symbol_name, position.line, position.character);

    // Search for the symbol in current file
    // Search for messages
    if let Some(msg) = find_message_recursive(&proto.messages, &symbol_name) {
        tracing::debug!("Hover: found message '{}'", msg.name);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format_message_hover(msg),
            }),
            range: None,
        });
    }

    // Search for enums
    if let Some(e) = find_enum_recursive(&proto.messages, &proto.enums, &symbol_name) {
        tracing::debug!("Hover: found enum '{}'", e.name);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format_enum_hover(e),
            }),
            range: None,
        });
    }

    // Search for services
    for svc in &proto.services {
        if svc.name == symbol_name {
            tracing::debug!("Hover: found service '{}'", svc.name);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format_service_hover(svc),
                }),
                range: None,
            });
        }
    }

    // Search in imported files
    for imported in collect_imports(&uri) {
        // Search for messages in imported file
        if let Some(msg) = find_message_recursive(&imported.messages, &symbol_name) {
            tracing::debug!("Hover: found message '{}' in imported file", msg.name);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format_message_hover(msg),
                }),
                range: None,
            });
        }

        // Search for enums in imported file
        if let Some(e) = find_enum_recursive(&imported.messages, &imported.enums, &symbol_name) {
            tracing::debug!("Hover: found enum '{}' in imported file", e.name);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format_enum_hover(e),
                }),
                range: None,
            });
        }

        // Search for services in imported file
        for svc in &imported.services {
            if svc.name == symbol_name {
                tracing::debug!("Hover: found service '{}' in imported file", svc.name);
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format_service_hover(svc),
                    }),
                    range: None,
                });
            }
        }
    }

    tracing::debug!("Hover: symbol '{}' not found", symbol_name);
    None
}

fn format_message_hover(msg: &MessageElement) -> String {
    let mut output = format!("**Message**: `{}`\n\n", msg.full_name);
    output.push_str("```protobuf\n");
    output.push_str(&format!("message {} {{\n", msg.name));

    for field in &msg.fields {
        output.push_str(&format!("  {} {} = {};\n", field.field_type, field.name, field.number));
    }

    if !msg.nested_messages.is_empty() {
        output.push_str("\n  // Nested messages\n");
        for nested in &msg.nested_messages {
            output.push_str(&format!("  message {} {{ ... }}\n", nested.name));
        }
    }

    if !msg.nested_enums.is_empty() {
        output.push_str("\n  // Nested enums\n");
        for nested_enum in &msg.nested_enums {
            output.push_str(&format!("  enum {} {{ ... }}\n", nested_enum.name));
        }
    }

    output.push_str("}\n```");
    output
}

fn format_enum_hover(e: &EnumElement) -> String {
    let mut output = format!("**Enum**: `{}`\n\n", e.full_name);
    output.push_str("```protobuf\n");
    output.push_str(&format!("enum {} {{\n", e.name));

    for value in &e.values {
        output.push_str(&format!("  {} = {};\n", value.name, value.number));
    }

    output.push_str("}\n```");
    output
}

fn format_service_hover(svc: &ServiceElement) -> String {
    let mut output = format!("**Service**: `{}`\n\n", svc.full_name);
    output.push_str("```protobuf\n");
    output.push_str(&format!("service {} {{\n", svc.name));

    for method in &svc.methods {
        output.push_str(&format!(
            "  rpc {}({}) returns ({});\n",
            method.name, method.input_type, method.output_type
        ));
    }

    output.push_str("}\n```");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{
        TextDocumentIdentifier, TextDocumentPositionParams,
    };

    /// Helper: build HoverParams for a given uri + position
    fn make_params(uri: &Url, line: u32, character: u32) -> HoverParams {
        HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
        }
    }

    /// Extract the markdown value from a Hover for easy assertion.
    fn hover_text(hover: Option<Hover>) -> String {
        match hover {
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent { value, .. }),
                ..
            }) => value,
            other => panic!("Expected markdown hover, got {:?}", other),
        }
    }

    // ---------------------------------------------------------------
    // Repro for: hover on a symbol defined in an imported file that
    // has NOT been opened in the editor.
    //
    //   imported.proto  →  message ImportedMessage { ... }
    //   main.proto      →  import "imported.proto";
    //                      message Main { ImportedMessage imported = 1; }
    //
    // Hovering on "ImportedMessage" in main.proto must return a hover
    // result even though imported.proto was never `did_open`-ed.
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_hover_on_imported_message_not_opened() {
        let dir = tempfile::tempdir().unwrap();

        let imported_content = r#"syntax = "proto3";
package common;

message ImportedMessage {
    string name = 1;
    int32 age = 2;
}
"#;
        let imported_path = dir.path().join("imported.proto");
        std::fs::write(&imported_path, imported_content).unwrap();
        // NOTE: intentionally NOT calling open_file on imported_uri —
        // the file exists on disk but is not in the workspace cache.

        let main_content = r#"syntax = "proto3";
package main;
import "imported.proto";

message Main {
    ImportedMessage imported = 1;
}
"#;
        let main_path = dir.path().join("main.proto");
        std::fs::write(&main_path, main_content).unwrap();
        let main_uri = Url::from_file_path(&main_path).unwrap();

        let ws = WorkspaceManager::new();
        ws.open_file(&main_uri, main_content).await.unwrap();

        // Cursor on "ImportedMessage" in line 5:
        //   "    ImportedMessage imported = 1;"
        let line = 5;
        let character = 6; // inside "ImportedMessage"

        let params = make_params(&main_uri, line, character);

        // Sync version: should FAIL (imported file not in cache) — this is the bug.
        let sync_result = provide_hover(params.clone(), &ws, Some(main_content));
        assert!(
            sync_result.is_none(),
            "Sync hover should return None when imported file is not cached, got: {:?}",
            sync_result
        );

        // Async version: should succeed by loading the imported file from disk.
        let async_result = provide_hover_async(params, &ws, Some(main_content)).await;
        assert!(async_result.is_some(), "Async hover should return a result");

        let text = hover_text(async_result);
        assert!(
            text.contains("ImportedMessage"),
            "Hover text should mention 'ImportedMessage', got: {}",
            text
        );
    }

    // ---------------------------------------------------------------
    // Sanity: when the imported file IS opened, both sync and async
    // versions should work.
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_hover_on_imported_message_opened() {
        let dir = tempfile::tempdir().unwrap();

        let imported_content = r#"syntax = "proto3";
package common;

message OpenedImport {
    string name = 1;
}
"#;
        let imported_path = dir.path().join("opened.proto");
        std::fs::write(&imported_path, imported_content).unwrap();
        let imported_uri = Url::from_file_path(&imported_path).unwrap();

        let main_content = r#"syntax = "proto3";
package main;
import "opened.proto";

message Main {
    OpenedImport imported = 1;
}
"#;
        let main_path = dir.path().join("main.proto");
        std::fs::write(&main_path, main_content).unwrap();
        let main_uri = Url::from_file_path(&main_path).unwrap();

        let ws = WorkspaceManager::new();
        ws.open_file(&imported_uri, imported_content).await.unwrap();
        ws.open_file(&main_uri, main_content).await.unwrap();

        // Cursor on "OpenedImport" in line 5
        let params = make_params(&main_uri, 5, 6);

        let sync_result = provide_hover(params.clone(), &ws, Some(main_content));
        assert!(sync_result.is_some(), "Sync hover should work when import is cached");

        let async_result = provide_hover_async(params, &ws, Some(main_content)).await;
        assert!(async_result.is_some(), "Async hover should work when import is cached");
    }
}
