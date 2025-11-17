use crate::workspace::WorkspaceManager;
use anyhow::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url,
};
use tower_lsp::Client;
use tracing::{debug, error, info};

pub async fn publish_diagnostics(
    uri: &Url,
    diagnostics: Vec<Diagnostic>,
    client: &Client,
) {
    if diagnostics.is_empty() {
        debug!("Clearing all diagnostics for {}", uri);
        // Publish empty diagnostics array to clear previous errors
        client.publish_diagnostics(uri.clone(), Vec::new(), None).await;
        return;
    }

    info!("Publishing {} diagnostics for {}", diagnostics.len(), uri);

    // Send diagnostics to client
    let diagnostics_count = diagnostics.len();
    client.publish_diagnostics(uri.clone(), diagnostics, None).await;
    debug!("Published {} diagnostics for {}", diagnostics_count, uri);
}

pub async fn validate_proto_file(uri: &Url, workspace: &WorkspaceManager, client: &Client) -> Result<()> {
    debug!("Validating proto file: {}", uri);

    let mut diagnostics = Vec::new();

    // Get the parsed proto file
    if let Some(proto) = workspace.get_file(uri) {
        // Check for syntax errors collected during parsing
        diagnostics.extend(validate_syntax(&proto));

        // Check for semantic issues
        diagnostics.extend(validate_semantics(&proto));

        // Add parse errors from the parser
        for parse_error in &proto.parse_errors {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: parse_error.line,
                        character: parse_error.character,
                    },
                    end: Position {
                        line: parse_error.line,
                        character: parse_error.character + 10, // Arbitrary end position
                    },
                },
                severity: Some(match parse_error.severity {
                    crate::parser::ErrorSeverity::Error => DiagnosticSeverity::ERROR,
                    crate::parser::ErrorSeverity::Warning => DiagnosticSeverity::WARNING,
                    crate::parser::ErrorSeverity::Info => DiagnosticSeverity::INFORMATION,
                }),
                code: Some(NumberOrString::String("syntax-error".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: parse_error.message.clone(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }
    }

    publish_diagnostics(uri, diagnostics, client).await;
    Ok(())
}

fn validate_syntax(proto: &crate::parser::ParsedProto) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // If we have no messages, enums, or services, it might be an empty file or syntax error
    if proto.messages.is_empty() && proto.enums.is_empty() && proto.services.is_empty() {
        // Check if file has content but no parsed elements
        if let Some(content) = get_file_content(&proto.uri) {
            if !content.trim().is_empty() && !content.contains("syntax") {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position::default(),
                        end: Position {
                            line: 0,
                            character: u32::MAX,
                        },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("missing-syntax".to_string())),
                    source: Some("protobuf-lsp".to_string()),
                    message: "Missing syntax declaration. Consider adding 'syntax = \"proto3\";' at the beginning of the file.".to_string(),
                    related_information: None,
                    tags: None,
                    code_description: None,
                    data: None,
                });
            }
        }
    }

    diagnostics
}

fn validate_semantics(proto: &crate::parser::ParsedProto) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check for duplicate message names
    let mut message_names = std::collections::HashSet::new();
    for msg in &proto.messages {
        if !message_names.insert(msg.name.clone()) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: msg.line,
                        character: msg.character,
                    },
                    end: Position {
                        line: msg.line,
                        character: msg.character + msg.name.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("duplicate-message".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: format!("Duplicate message name: '{}'", msg.name),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }
    }

    // Check for duplicate enum names
    let mut enum_names = std::collections::HashSet::new();
    for e in &proto.enums {
        if !enum_names.insert(e.name.clone()) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: e.line,
                        character: e.character,
                    },
                    end: Position {
                        line: e.line,
                        character: e.character + e.name.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("duplicate-enum".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: format!("Duplicate enum name: '{}'", e.name),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }
    }

    // Check for duplicate service names
    let mut service_names = std::collections::HashSet::new();
    for svc in &proto.services {
        if !service_names.insert(svc.name.clone()) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: svc.line,
                        character: svc.character,
                    },
                    end: Position {
                        line: svc.line,
                        character: svc.character + svc.name.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("duplicate-service".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: format!("Duplicate service name: '{}'", svc.name),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }
    }

    // Check for field number conflicts within messages
    for msg in &proto.messages {
        let mut field_numbers = std::collections::HashMap::new();
        for field in &msg.fields {
            if let Some(existing_line) = field_numbers.get(&field.number) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: field.line,
                            character: field.character,
                        },
                        end: Position {
                            line: field.line,
                            character: field.character + field.name.len() as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("duplicate-field-number".to_string())),
                    source: Some("protobuf-lsp".to_string()),
                    message: format!(
                        "Field number {} is already used in this message (first used at line {})",
                        field.number,
                        existing_line + 1
                    ),
                    related_information: None,
                    tags: None,
                    code_description: None,
                    data: None,
                });
            } else {
                field_numbers.insert(field.number, field.line);
            }
        }
    }

    diagnostics
}

fn get_file_content(uri: &str) -> Option<String> {
    use std::fs;
    use std::path::Path;

    // Convert URI to file path
    if uri.starts_with("file://") {
        let path = uri.trim_start_matches("file://");
        if Path::new(path).exists() {
            fs::read_to_string(path).ok()
        } else {
            None
        }
    } else {
        None
    }
}

// Parse errors from protobuf-parse library
pub fn create_parse_diagnostics(
    uri: &Url,
    parse_result: &Result<crate::parser::ParsedProto>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Err(e) = parse_result {
        error!("Parse error for {}: {}", uri, e);

        // Try to extract line information from the error message
        let error_str = e.to_string();
        if let Some(line_info) = extract_line_from_error(&error_str) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: line_info,
                        character: 0,
                    },
                    end: Position {
                        line: line_info,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("parse-error".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: format!("Parse error: {}", error_str),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        } else {
            // If we can't extract line info, show error at the beginning of the file
            diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("parse-error".to_string())),
                source: Some("protobuf-lsp".to_string()),
                message: format!("Parse error: {}", error_str),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }
    }

    diagnostics
}

fn extract_line_from_error(error_str: &str) -> Option<u32> {
    // Common patterns for line numbers in error messages
    // Look for patterns like "line X:", "at line X", "L:X", etc.
    use regex::Regex;

    let patterns = [
        r"line\s+(\d+):",
        r"at line (\d+)",
        r"L:(\d+)",
        r"line\s+(\d+)",
        r":(\d+):\d+:",  // GCC-style: file:line:column:
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(error_str) {
                if let Some(line_match) = caps.get(1) {
                    if let Ok(line_num) = line_match.as_str().parse::<u32>() {
                        return Some(line_num.saturating_sub(1)); // Convert to 0-indexed
                    }
                }
            }
        }
    }

    None
}