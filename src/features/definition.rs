use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range, Url};

/// Extract the word at the given position from the content
fn extract_word_at_position(content: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        tracing::debug!("Line {} out of range (total: {})", position.line, lines.len());
        return None;
    }

    let line = lines[position.line as usize];
    let char_pos = position.character as usize;

    tracing::debug!("Extracting word from line {}: '{}', char_pos: {}", position.line, line, char_pos);

    if char_pos > line.len() {
        tracing::debug!("Character position {} out of range (line length: {})", char_pos, line.len());
        return None;
    }

    // Find word boundaries
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        tracing::debug!("Empty line");
        return None;
    }

    // Handle cursor at end of line or beyond - try the character before
    let mut check_pos = if char_pos >= chars.len() && char_pos > 0 {
        char_pos - 1
    } else if char_pos >= chars.len() {
        tracing::debug!("Character position at or beyond end of line");
        return None;
    } else {
        char_pos
    };

    // Check if current position is on a word character, if not try the previous character
    if !chars[check_pos].is_alphanumeric() && chars[check_pos] != '_' && chars[check_pos] != '.' {
        if check_pos > 0 && (chars[check_pos - 1].is_alphanumeric() || chars[check_pos - 1] == '_' || chars[check_pos - 1] == '.') {
            check_pos -= 1;
        } else {
            tracing::debug!("No word character at position {}", check_pos);
            return None;
        }
    }

    // Find start of word (including dots for qualified names)
    let mut start = check_pos;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '.') {
        start -= 1;
    }

    // Find end of word (including dots for qualified names)
    let mut end = check_pos;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '.') {
        end += 1;
    }

    let word: String = chars[start..end].iter().collect();
    tracing::debug!("Extracted word: '{}'", word);
    Some(word)
}

/// Extract the import path if the cursor is on an import statement
fn extract_import_path_at_position(content: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let trimmed = line.trim();

    // Check if this line is an import statement
    if !trimmed.starts_with("import ") {
        return None;
    }

    // Extract the import path from quotes
    if let Some(start_quote) = line.find('"') {
        if let Some(end_quote) = line.rfind('"') {
            if start_quote < end_quote {
                let char_pos = position.character as usize;

                // Check if cursor is within the quotes
                if char_pos >= start_quote && char_pos <= end_quote {
                    let import_path = &line[start_quote + 1..end_quote];
                    tracing::debug!("Extracted import path: '{}'", import_path);
                    return Some(import_path.to_string());
                }
            }
        }
    }

    None
}

pub fn provide_definition(
    params: GotoDefinitionParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let proto = workspace.get_file(&uri)?;
    let content = content?;

    // First check if the cursor is on an import statement
    if let Some(import_path) = extract_import_path_at_position(content, position) {
        tracing::debug!("Cursor is on import path: '{}'", import_path);

        // Try to resolve the import path (only cached files for sync version)
        if let Some(imported_file) = workspace.get_imported_file_cached(&uri, &import_path) {
            let import_uri = Url::parse(&imported_file.uri).ok()?;

            // Return a location pointing to the beginning of the imported file
            let location = Location {
                uri: import_uri,
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
            };
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    // Extract the word at the cursor position
    let symbol_name = if let Some(word) = extract_word_at_position(content, position) {
        tracing::debug!("Extracted symbol name: '{}' at position {}:{}", word, position.line, position.character);
        word
    } else {
        return None;
    };

    // Split qualified name into package and simple name
    let (package_prefix, simple_name): (Option<&str>, String) = if symbol_name.contains('.') {
        if let Some(last_dot) = symbol_name.rfind('.') {
            let package = &symbol_name[..last_dot];
            let name = symbol_name[last_dot + 1..].to_string();
            (Some(package), name)
        } else {
            (None, symbol_name)
        }
    } else {
        (None, symbol_name)
    };

    tracing::debug!("Package prefix: {:?}, Simple name: '{}'", package_prefix, simple_name);

    // Helper function to create location for a message
    let create_message_location = |msg: &crate::parser::proto::MessageElement, file_uri: &Url| -> Location {
        Location {
            uri: file_uri.clone(),
            range: Range {
                start: Position {
                    line: msg.line,
                    character: msg.character + "message ".len() as u32,
                },
                end: Position {
                    line: msg.line,
                    character: msg.character + "message ".len() as u32 + msg.name.len() as u32,
                },
            },
        }
    };

    // Helper function to create location for an enum
    let create_enum_location = |e: &crate::parser::proto::EnumElement, file_uri: &Url| -> Location {
        Location {
            uri: file_uri.clone(),
            range: Range {
                start: Position {
                    line: e.line,
                    character: e.character + "enum ".len() as u32,
                },
                end: Position {
                    line: e.line,
                    character: e.character + "enum ".len() as u32 + e.name.len() as u32,
                },
            },
        }
    };

    // Helper function to check if a message matches by name and optionally by package
    fn matches_message(msg: &crate::parser::proto::MessageElement, name: &str, package: Option<&str>) -> bool {
        if msg.name != name {
            return false;
        }

        if let Some(pkg) = package {
            // Check if the message's full_name matches the expected package.name format
            msg.full_name == format!("{}.{}", pkg, name)
        } else {
            true
        }
    }

    // Helper function to check if an enum matches by name and optionally by package
    fn matches_enum(e: &crate::parser::proto::EnumElement, name: &str, package: Option<&str>) -> bool {
        if e.name != name {
            return false;
        }

        if let Some(pkg) = package {
            e.full_name == format!("{}.{}", pkg, name)
        } else {
            true
        }
    }

    // Search in current file first
    // Search for messages
    tracing::debug!("Searching for message '{}' (package: {:?}) in {} messages", simple_name, package_prefix, proto.messages.len());
    for (i, msg) in proto.messages.iter().enumerate() {
        tracing::debug!("  Message[{}]: '{}' (full: '{}') at line {}", i, msg.name, msg.full_name, msg.line);
    }

    // Try to find by simple name first
    if let Some(msg) = proto.find_message_by_name(&simple_name) {
        // If we have a package prefix, verify it matches
        if package_prefix.is_none() || matches_message(msg, &simple_name, package_prefix) {
            tracing::debug!("Found message '{}' at line {}", msg.name, msg.line);
            return Some(GotoDefinitionResponse::Scalar(create_message_location(msg, &uri)));
        }
    }

    // Search for enums
    if let Some(e) = proto.find_enum_by_name(&simple_name) {
        // If we have a package prefix, verify it matches
        if package_prefix.is_none() || matches_enum(e, &simple_name, package_prefix) {
            return Some(GotoDefinitionResponse::Scalar(create_enum_location(e, &uri)));
        }
    }

    // Search for services
    if let Some(svc) = proto.find_service_by_name(&simple_name) {
        let location = Location {
            uri: uri.clone(),
            range: Range {
                start: Position {
                    line: svc.line,
                    character: svc.character + "service ".len() as u32,
                },
                end: Position {
                    line: svc.line,
                    character: svc.character + "service ".len() as u32 + svc.name.len() as u32,
                },
            },
        };
        return Some(GotoDefinitionResponse::Scalar(location));
    }

    // Search in imported files (only cached files for sync version)
    for import in &proto.imports {
        if let Some(imported) = workspace.get_imported_file_cached(&uri, &import.path) {
            let import_uri = Url::parse(&imported.uri).ok()?;

            // Search for messages in imported file
            if let Some(msg) = imported.find_message_by_name(&simple_name) {
                // If we have a package prefix, verify it matches
                if package_prefix.is_none() || matches_message(msg, &simple_name, package_prefix) {
                    tracing::debug!("Found message '{}' in imported file {}", msg.name, imported.uri);
                    return Some(GotoDefinitionResponse::Scalar(create_message_location(msg, &import_uri)));
                }
            }

            // Search for enums in imported file
            if let Some(e) = imported.find_enum_by_name(&simple_name) {
                // If we have a package prefix, verify it matches
                if package_prefix.is_none() || matches_enum(e, &simple_name, package_prefix) {
                    return Some(GotoDefinitionResponse::Scalar(create_enum_location(e, &import_uri)));
                }
            }

            // Search for services in imported file
            if let Some(svc) = imported.find_service_by_name(&simple_name) {
                let location = Location {
                    uri: import_uri.clone(),
                    range: Range {
                        start: Position {
                            line: svc.line,
                            character: svc.character + "service ".len() as u32,
                        },
                        end: Position {
                            line: svc.line,
                            character: svc.character + "service ".len() as u32 + svc.name.len() as u32,
                        },
                    },
                };
                return Some(GotoDefinitionResponse::Scalar(location));
            }
        }
    }

    None
}

/// Async version that can load imported files on demand
pub async fn provide_definition_async(
    params: GotoDefinitionParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<GotoDefinitionResponse> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let proto = workspace.get_file(&uri)?;
    let content = content?;

    // First check if the cursor is on an import statement
    if let Some(import_path) = extract_import_path_at_position(content, position) {
        tracing::debug!("Cursor is on import path: '{}'", import_path);

        // Try to resolve the import path (async version can load files)
        if let Some(imported_file) = workspace.get_imported_file(&uri, &import_path).await
            .or_else(|| workspace.get_imported_file_cached(&uri, &import_path)) {
            let import_uri = Url::parse(&imported_file.uri).ok()?;

            // Return a location pointing to the beginning of the imported file
            let location = Location {
                uri: import_uri,
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
            };
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    // Extract the word at the cursor position
    let symbol_name = if let Some(word) = extract_word_at_position(content, position) {
        tracing::debug!("Extracted symbol name: '{}' at position {}:{}", word, position.line, position.character);
        word
    } else {
        return None;
    };

    // Split qualified name into package and simple name
    let (package_prefix, simple_name): (Option<&str>, String) = if symbol_name.contains('.') {
        if let Some(last_dot) = symbol_name.rfind('.') {
            let package = &symbol_name[..last_dot];
            let name = symbol_name[last_dot + 1..].to_string();
            (Some(package), name)
        } else {
            (None, symbol_name)
        }
    } else {
        (None, symbol_name)
    };

    tracing::debug!("Package prefix: {:?}, Simple name: '{}'", package_prefix, simple_name);

    // Helper function to create location for a message
    let create_message_location = |msg: &crate::parser::proto::MessageElement, file_uri: &Url| -> Location {
        Location {
            uri: file_uri.clone(),
            range: Range {
                start: Position {
                    line: msg.line,
                    character: msg.character + "message ".len() as u32,
                },
                end: Position {
                    line: msg.line,
                    character: msg.character + "message ".len() as u32 + msg.name.len() as u32,
                },
            },
        }
    };

    // Helper function to create location for an enum
    let create_enum_location = |e: &crate::parser::proto::EnumElement, file_uri: &Url| -> Location {
        Location {
            uri: file_uri.clone(),
            range: Range {
                start: Position {
                    line: e.line,
                    character: e.character + "enum ".len() as u32,
                },
                end: Position {
                    line: e.line,
                    character: e.character + "enum ".len() as u32 + e.name.len() as u32,
                },
            },
        }
    };

    // Helper function to check if a message matches by name and optionally by package
    fn matches_message(msg: &crate::parser::proto::MessageElement, name: &str, package: Option<&str>) -> bool {
        if msg.name != name {
            return false;
        }

        if let Some(pkg) = package {
            // Check if the message's full_name matches the expected package.name format
            msg.full_name == format!("{}.{}", pkg, name)
        } else {
            true
        }
    }

    // Helper function to check if an enum matches by name and optionally by package
    fn matches_enum(e: &crate::parser::proto::EnumElement, name: &str, package: Option<&str>) -> bool {
        if e.name != name {
            return false;
        }

        if let Some(pkg) = package {
            e.full_name == format!("{}.{}", pkg, name)
        } else {
            true
        }
    }

    // Search in current file first
    if let Some(msg) = proto.find_message_by_name(&simple_name) {
        // If we have a package prefix, verify it matches
        if package_prefix.is_none() || matches_message(msg, &simple_name, package_prefix) {
            tracing::debug!("Found message '{}' at line {}", msg.name, msg.line);
            return Some(GotoDefinitionResponse::Scalar(create_message_location(msg, &uri)));
        }
    }

    // Search for enums
    if let Some(e) = proto.find_enum_by_name(&simple_name) {
        // If we have a package prefix, verify it matches
        if package_prefix.is_none() || matches_enum(e, &simple_name, package_prefix) {
            return Some(GotoDefinitionResponse::Scalar(create_enum_location(e, &uri)));
        }
    }

    // Search for services
    if let Some(svc) = proto.find_service_by_name(&simple_name) {
        let location = Location {
            uri: uri.clone(),
            range: Range {
                start: Position {
                    line: svc.line,
                    character: svc.character + "service ".len() as u32,
                },
                end: Position {
                    line: svc.line,
                    character: svc.character + "service ".len() as u32 + svc.name.len() as u32,
                },
            },
        };
        return Some(GotoDefinitionResponse::Scalar(location));
    }

    // Search in all recursively imported files
    let all_imports = workspace.collect_all_imports_async(&uri).await;
    tracing::debug!("Searching in {} recursively imported files", all_imports.len());

    for imported in &all_imports {
        let import_uri = Url::parse(&imported.uri).ok()?;

        // Search for messages in imported file
        if let Some(msg) = imported.find_message_by_name(&simple_name) {
            // If we have a package prefix, verify it matches
            if package_prefix.is_none() || matches_message(msg, &simple_name, package_prefix) {
                tracing::debug!("Found message '{}' in imported file {}", msg.name, imported.uri);
                return Some(GotoDefinitionResponse::Scalar(create_message_location(msg, &import_uri)));
            }
        }

        // Search for enums in imported file
        if let Some(e) = imported.find_enum_by_name(&simple_name) {
            // If we have a package prefix, verify it matches
            if package_prefix.is_none() || matches_enum(e, &simple_name, package_prefix) {
                return Some(GotoDefinitionResponse::Scalar(create_enum_location(e, &import_uri)));
            }
        }

        // Search for services in imported file
        if let Some(svc) = imported.find_service_by_name(&simple_name) {
            let location = Location {
                uri: import_uri.clone(),
                range: Range {
                    start: Position {
                        line: svc.line,
                        character: svc.character + "service ".len() as u32,
                    },
                    end: Position {
                        line: svc.line,
                        character: svc.character + "service ".len() as u32 + svc.name.len() as u32,
                    },
                },
            };
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    None
}