use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Provide code actions (quick fixes) for diagnostics and source actions.
pub fn provide_code_actions(
    params: CodeActionParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<Vec<CodeActionOrCommand>> {
    let uri = &params.text_document.uri;
    let content = content?;
    let mut actions = Vec::new();

    // Quick fixes based on diagnostics
    for diag in &params.context.diagnostics {
        if let Some(code) = &diag.code {
            match code {
                NumberOrString::String(s) if s == "missing-syntax" => {
                    actions.push(create_insert_syntax_action(uri));
                }
                NumberOrString::String(s) if s == "duplicate-field-number" => {
                    if let Some(action) =
                        create_fix_field_number_action(uri, diag, workspace, content)
                    {
                        actions.push(action);
                    }
                }
                _ => {}
            }
        }
    }

    // Source action: sort imports
    if has_imports(content) {
        if let Some(action) = create_sort_imports_action(uri, content) {
            actions.push(action);
        }
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

/// Create a code action to insert `syntax = "proto3";` at the top of the file.
fn create_insert_syntax_action(uri: &Url) -> CodeActionOrCommand {
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: "syntax = \"proto3\";\n\n".to_string(),
        }],
    );

    CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add syntax = \"proto3\" declaration".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Create a code action to fix a duplicate field number by suggesting the next available number.
fn create_fix_field_number_action(
    uri: &Url,
    diag: &Diagnostic,
    workspace: &WorkspaceManager,
    content: &str,
) -> Option<CodeActionOrCommand> {
    let proto = workspace.get_file(uri)?;
    let diag_line = diag.range.start.line;

    // Find which message this field belongs to and determine next available number
    for msg in &proto.messages {
        if diag_line >= msg.line && diag_line <= msg.end_line {
            let max_number = msg.fields.iter().map(|f| f.number).max().unwrap_or(0);
            let next_number = max_number + 1;

            // Find the field number in the line text
            let line_str = content.lines().nth(diag_line as usize)?;

            // Find "= N" pattern and replace N
            if let Some(eq_pos) = line_str.find('=') {
                let after_eq = line_str[eq_pos + 1..].trim_start();
                let num_end = after_eq
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(after_eq.len());
                let num_str = &after_eq[..num_end];

                if !num_str.is_empty() {
                    let num_start_in_line =
                        eq_pos + 1 + (line_str[eq_pos + 1..].len() - after_eq.len());
                    let num_end_in_line = num_start_in_line + num_end;

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range {
                                start: Position {
                                    line: diag_line,
                                    character: num_start_in_line as u32,
                                },
                                end: Position {
                                    line: diag_line,
                                    character: num_end_in_line as u32,
                                },
                            },
                            new_text: next_number.to_string(),
                        }],
                    );

                    return Some(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Change field number to {}", next_number),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            document_changes: None,
                            change_annotations: None,
                        }),
                        command: None,
                        is_preferred: Some(true),
                        disabled: None,
                        data: None,
                    }));
                }
            }
        }
    }

    None
}

/// Check if the file has import statements.
fn has_imports(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.trim().starts_with("import "))
}

/// Create a source action to sort import statements alphabetically.
fn create_sort_imports_action(uri: &Url, content: &str) -> Option<CodeActionOrCommand> {
    let lines: Vec<&str> = content.lines().collect();

    // Collect contiguous import blocks
    let mut import_start: Option<usize> = None;
    let mut import_lines: Vec<(usize, &str)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("import ") {
            if import_start.is_none() {
                import_start = Some(i);
            }
            import_lines.push((i, line));
        } else if import_start.is_some() && !line.trim().is_empty() {
            break;
        }
    }

    if import_lines.len() < 2 {
        return None;
    }

    // Sort imports alphabetically
    let mut sorted_imports: Vec<String> = import_lines.iter().map(|(_, l)| l.to_string()).collect();
    sorted_imports.sort();

    // Check if already sorted
    let original: Vec<String> = import_lines.iter().map(|(_, l)| l.to_string()).collect();
    if original == sorted_imports {
        return None;
    }

    let first_line = import_lines.first()?.0;
    let last_line = import_lines.last()?.0;

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: first_line as u32,
                    character: 0,
                },
                end: Position {
                    line: last_line as u32,
                    character: lines[last_line].len() as u32,
                },
            },
            new_text: sorted_imports.join("\n"),
        }],
    );

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Sort import statements".to_string(),
        kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_imports() {
        assert!(has_imports("import \"foo.proto\";\nimessage Foo {}"));
        assert!(!has_imports("message Foo {}"));
    }

    #[test]
    fn test_sort_imports_action() {
        let content = r#"syntax = "proto3";

import "c.proto";
import "a.proto";
import "b.proto";

message Foo {}
"#;
        let uri = Url::parse("file:///test.proto").unwrap();
        let action = create_sort_imports_action(&uri, content);
        assert!(action.is_some());
    }

    #[test]
    fn test_already_sorted_no_action() {
        let content = r#"syntax = "proto3";

import "a.proto";
import "b.proto";
import "c.proto";

message Foo {}
"#;
        let uri = Url::parse("file:///test.proto").unwrap();
        let action = create_sort_imports_action(&uri, content);
        assert!(action.is_none());
    }
}
