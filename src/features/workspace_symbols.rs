use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Search for symbols across all open files matching the query string.
/// Supports case-insensitive substring matching.
pub fn workspace_symbol(
    params: WorkspaceSymbolParams,
    workspace: &WorkspaceManager,
) -> Option<Vec<SymbolInformation>> {
    let query = params.query.to_lowercase();
    let mut results = Vec::new();

    let all_files = workspace.get_all_files();

    for (uri_str, proto) in &all_files {
        let uri = match Url::parse(uri_str) {
            Ok(u) => u,
            Err(_) => continue,
        };

        // Search messages
        for msg in &proto.messages {
            if matches_query(&msg.name, &query) {
                results.push(make_symbol_info(
                    &msg.name,
                    SymbolKind::STRUCT,
                    &uri,
                    msg.line,
                    msg.character,
                    msg.end_line,
                    proto.package.as_deref(),
                ));
            }
            // Search nested messages
            collect_nested_message_symbols(msg, &uri, &query, proto.package.as_deref(), &mut results);
        }

        // Search enums
        for e in &proto.enums {
            if matches_query(&e.name, &query) {
                results.push(make_symbol_info(
                    &e.name,
                    SymbolKind::ENUM,
                    &uri,
                    e.line,
                    e.character,
                    e.end_line,
                    proto.package.as_deref(),
                ));
            }
            // Search enum values
            for val in &e.values {
                if matches_query(&val.name, &query) {
                    results.push(make_symbol_info(
                        &val.name,
                        SymbolKind::ENUM_MEMBER,
                        &uri,
                        val.line,
                        val.character,
                        val.line,
                        Some(&e.name),
                    ));
                }
            }
        }

        // Search services
        for svc in &proto.services {
            if matches_query(&svc.name, &query) {
                results.push(make_symbol_info(
                    &svc.name,
                    SymbolKind::INTERFACE,
                    &uri,
                    svc.line,
                    svc.character,
                    svc.end_line,
                    proto.package.as_deref(),
                ));
            }
            // Search methods
            for method in &svc.methods {
                if matches_query(&method.name, &query) {
                    results.push(make_symbol_info(
                        &method.name,
                        SymbolKind::METHOD,
                        &uri,
                        method.line,
                        method.character,
                        method.line,
                        Some(&svc.name),
                    ));
                }
            }
        }
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

fn collect_nested_message_symbols(
    msg: &crate::parser::MessageElement,
    uri: &Url,
    query: &str,
    _container: Option<&str>,
    results: &mut Vec<SymbolInformation>,
) {
    for nested in &msg.nested_messages {
        if matches_query(&nested.name, query) {
            results.push(make_symbol_info(
                &nested.name,
                SymbolKind::STRUCT,
                uri,
                nested.line,
                nested.character,
                nested.end_line,
                Some(&msg.name),
            ));
        }
        collect_nested_message_symbols(nested, uri, query, Some(&msg.name), results);
    }
    for nested_enum in &msg.nested_enums {
        if matches_query(&nested_enum.name, query) {
            results.push(make_symbol_info(
                &nested_enum.name,
                SymbolKind::ENUM,
                uri,
                nested_enum.line,
                nested_enum.character,
                nested_enum.end_line,
                Some(&msg.name),
            ));
        }
    }
}

/// Case-insensitive substring match. Empty query matches everything.
fn matches_query(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    name.to_lowercase().contains(query)
}

#[allow(deprecated)]
fn make_symbol_info(
    name: &str,
    kind: SymbolKind,
    uri: &Url,
    line: u32,
    character: u32,
    end_line: u32,
    container_name: Option<&str>,
) -> SymbolInformation {
    SymbolInformation {
        name: name.to_string(),
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: Range {
                start: Position { line, character },
                end: Position {
                    line: end_line,
                    character: 0,
                },
            },
        },
        container_name: container_name.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_query() {
        assert!(matches_query("UserRequest", "user"));
        assert!(matches_query("UserRequest", "request"));
        assert!(matches_query("UserRequest", ""));
        assert!(!matches_query("UserRequest", "xyz"));
        assert!(matches_query("UserRequest", "userrequest")); // case insensitive
    }
}
