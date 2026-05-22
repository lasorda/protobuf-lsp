use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Position, Range, SymbolKind,
};

#[allow(deprecated)]
pub fn provide_document_symbols(
    params: DocumentSymbolParams,
    workspace: &WorkspaceManager,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;
    let proto = workspace.get_file(&uri)?;

    let mut symbols = Vec::new();

    // Add package
    if let Some(pkg) = &proto.package {
        symbols.push(DocumentSymbol {
            name: pkg.clone(),
            detail: Some("package".to_string()),
            kind: SymbolKind::PACKAGE,
            range: Range::default(),
            selection_range: Range::default(),
            children: None,
            tags: None,
            deprecated: None,
        });
    }

    // Add imports
    for import in &proto.imports {
        symbols.push(DocumentSymbol {
            name: import.path.clone(),
            detail: Some(format!("import (line {})", import.line + 1)),
            kind: SymbolKind::FILE,
            range: Range {
                start: Position {
                    line: import.line,
                    character: import.character,
                },
                end: Position {
                    line: import.line,
                    character: import.character + import.path.len() as u32,
                },
            },
            selection_range: Range {
                start: Position {
                    line: import.line,
                    character: import.character,
                },
                end: Position {
                    line: import.line,
                    character: import.character + import.path.len() as u32,
                },
            },
            children: None,
            tags: None,
            deprecated: None,
        });
    }

    // Add messages
    for msg in &proto.messages {
        symbols.push(create_message_symbol(msg));
    }

    // Add enums
    for e in &proto.enums {
        symbols.push(create_enum_symbol(e));
    }

    // Add services
    for svc in &proto.services {
        symbols.push(create_service_symbol(svc));
    }

    Some(DocumentSymbolResponse::Nested(symbols))
}

#[allow(deprecated)]
fn create_message_symbol(msg: &crate::parser::proto::MessageElement) -> DocumentSymbol {
    let mut children = Vec::new();

    // Don't add fields as children - only show nested messages and enums

    // Add nested messages as children
    for nested in &msg.nested_messages {
        children.push(create_message_symbol(nested));
    }

    // Add nested enums as children
    for nested_enum in &msg.nested_enums {
        children.push(create_enum_symbol(nested_enum));
    }

    // msg.character already points to the name (past the "message " keyword),
    // so we subtract to get the keyword start position for the full range.
    let keyword_start = msg.character.saturating_sub("message ".len() as u32);

    DocumentSymbol {
        name: msg.name.clone(),
        detail: Some(format!("line {}", msg.line + 1)), // Show line number (1-indexed for display)
        kind: SymbolKind::CLASS,
        range: Range {
            start: Position {
                line: msg.line,
                character: keyword_start,
            },
            end: Position {
                line: msg.end_line,
                character: 0,
            },
        },
        selection_range: Range {
            start: Position {
                line: msg.line,
                character: msg.character,
            },
            end: Position {
                line: msg.line,
                character: msg.character + msg.name.len() as u32,
            },
        },
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
        tags: None,
        deprecated: None,
    }
}

#[allow(deprecated)]
fn create_enum_symbol(e: &crate::parser::proto::EnumElement) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> = e
        .values
        .iter()
        .map(|value| DocumentSymbol {
            name: value.name.clone(),
            detail: Some(format!("= {} (line {})", value.number, value.line + 1)),
            kind: SymbolKind::ENUM_MEMBER,
            range: Range {
                start: Position {
                    line: value.line,
                    character: value.character,
                },
                end: Position {
                    line: value.line,
                    character: value.character + value.name.len() as u32,
                },
            },
            selection_range: Range {
                start: Position {
                    line: value.line,
                    character: value.character,
                },
                end: Position {
                    line: value.line,
                    character: value.character + value.name.len() as u32,
                },
            },
            children: None,
            tags: None,
            deprecated: None,
        })
        .collect();

    // e.character already points to the name (past the "enum " keyword),
    // so we subtract to get the keyword start position for the full range.
    let keyword_start = e.character.saturating_sub("enum ".len() as u32);

    DocumentSymbol {
        name: e.name.clone(),
        detail: Some(format!("line {}", e.line + 1)), // Show line number
        kind: SymbolKind::ENUM,
        range: Range {
            start: Position {
                line: e.line,
                character: keyword_start,
            },
            end: Position {
                line: e.end_line,
                character: 0,
            },
        },
        selection_range: Range {
            start: Position {
                line: e.line,
                character: e.character,
            },
            end: Position {
                line: e.line,
                character: e.character + e.name.len() as u32,
            },
        },
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
        tags: None,
        deprecated: None,
    }
}

#[allow(deprecated)]
fn create_service_symbol(svc: &crate::parser::proto::ServiceElement) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> = svc
        .methods
        .iter()
        .map(|method| {
            // method.character already points to the name (past the "rpc " keyword)
            let keyword_start = method.character.saturating_sub("rpc ".len() as u32);
            DocumentSymbol {
                name: method.name.clone(),
                detail: Some(format!("({}) returns ({}) (line {})", method.input_type, method.output_type, method.line + 1)),
                kind: SymbolKind::METHOD,
                range: Range {
                    start: Position {
                        line: method.line,
                        character: keyword_start,
                    },
                    end: Position {
                        line: method.line,
                        character: method.character + method.name.len() as u32,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: method.line,
                        character: method.character,
                    },
                    end: Position {
                        line: method.line,
                        character: method.character + method.name.len() as u32,
                    },
                },
                children: None,
                tags: None,
                deprecated: None,
            }
        })
        .collect();

    // svc.character already points to the name (past the "service " keyword),
    // so we subtract to get the keyword start position for the full range.
    let keyword_start = svc.character.saturating_sub("service ".len() as u32);

    DocumentSymbol {
        name: svc.name.clone(),
        detail: Some(format!("line {}", svc.line + 1)), // Show line number
        kind: SymbolKind::INTERFACE,
        range: Range {
            start: Position {
                line: svc.line,
                character: keyword_start,
            },
            end: Position {
                line: svc.end_line,
                character: 0,
            },
        },
        selection_range: Range {
            start: Position {
                line: svc.line,
                character: svc.character,
            },
            end: Position {
                line: svc.line,
                character: svc.character + svc.name.len() as u32,
            },
        },
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
        tags: None,
        deprecated: None,
    }
}
