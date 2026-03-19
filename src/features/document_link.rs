use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Provide document links for import statements.
/// Makes `import "path/to/file.proto"` paths clickable.
pub fn provide_document_links(
    params: DocumentLinkParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<Vec<DocumentLink>> {
    let uri = &params.text_document.uri;
    let _proto = workspace.get_file(uri)?;
    let content = content?;

    let mut links = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") {
            continue;
        }

        // Extract the quoted path from import statement
        if let Some((path, start_col, end_col)) = extract_import_path(line) {
            // Try to resolve the import path to an actual file
            if let Some(resolved_path) = workspace.resolve_import(uri, &path) {
                if let Ok(target_uri) = Url::from_file_path(&resolved_path) {
                    links.push(DocumentLink {
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: start_col as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: end_col as u32,
                            },
                        },
                        target: Some(target_uri),
                        tooltip: Some(format!("Open {}", resolved_path.display())),
                        data: None,
                    });
                }
            }
        }
    }

    if links.is_empty() {
        None
    } else {
        Some(links)
    }
}

/// Extract the import path and its column range from an import line.
/// Returns (path, start_column, end_column) where columns include the quotes.
fn extract_import_path(line: &str) -> Option<(String, usize, usize)> {
    // Look for both double-quoted and single-quoted paths
    let quote_start = line.find('"').or_else(|| line.find('\''))?;
    let quote_char = line.as_bytes()[quote_start] as char;
    let after_quote = &line[quote_start + 1..];
    let quote_end_rel = after_quote.find(quote_char)?;
    let path = after_quote[..quote_end_rel].to_string();

    // Return the range including quotes
    let start_col = quote_start;
    let end_col = quote_start + 1 + quote_end_rel + 1;

    Some((path, start_col, end_col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_import_path() {
        let result = extract_import_path(r#"import "google/protobuf/timestamp.proto";"#);
        assert!(result.is_some());
        let (path, start, end) = result.unwrap();
        assert_eq!(path, "google/protobuf/timestamp.proto");
        assert_eq!(start, 7);  // position of first "
        assert_eq!(end, 40);   // position after last "
    }

    #[test]
    fn test_extract_import_path_single_quote() {
        let result = extract_import_path("import 'foo.proto';");
        assert!(result.is_some());
        let (path, _, _) = result.unwrap();
        assert_eq!(path, "foo.proto");
    }

    #[test]
    fn test_extract_import_path_no_quotes() {
        let result = extract_import_path("import foo.proto;");
        assert!(result.is_none());
    }
}
