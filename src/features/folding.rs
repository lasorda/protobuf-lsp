use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Provide folding ranges for message, enum, service, oneof blocks,
/// contiguous import statements, and multi-line comments.
pub fn provide_folding_ranges(
    params: FoldingRangeParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<Vec<FoldingRange>> {
    let uri = &params.text_document.uri;
    let proto = workspace.get_file(uri)?;
    let content = content?;

    let mut ranges = Vec::new();

    // Fold message blocks
    for msg in &proto.messages {
        add_message_folding(msg, &mut ranges);
    }

    // Fold enum blocks
    for e in &proto.enums {
        if e.end_line > e.line {
            ranges.push(FoldingRange {
                start_line: e.line,
                start_character: None,
                end_line: e.end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(format!("enum {} {{ ... }}", e.name)),
            });
        }
    }

    // Fold service blocks
    for svc in &proto.services {
        if svc.end_line > svc.line {
            ranges.push(FoldingRange {
                start_line: svc.line,
                start_character: None,
                end_line: svc.end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(format!("service {} {{ ... }}", svc.name)),
            });
        }
    }

    // Fold contiguous import statements
    if let Some(import_range) = find_import_range(content) {
        if import_range.1 > import_range.0 {
            ranges.push(FoldingRange {
                start_line: import_range.0,
                start_character: None,
                end_line: import_range.1,
                end_character: None,
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: Some("imports ...".to_string()),
            });
        }
    }

    // Fold multi-line comments
    ranges.extend(find_comment_ranges(content));

    // Fold oneof blocks (scan content for oneof { ... })
    ranges.extend(find_oneof_ranges(content));

    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

/// Recursively add folding ranges for messages and their nested messages.
fn add_message_folding(msg: &crate::parser::MessageElement, ranges: &mut Vec<FoldingRange>) {
    if msg.end_line > msg.line {
        ranges.push(FoldingRange {
            start_line: msg.line,
            start_character: None,
            end_line: msg.end_line,
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: Some(format!("message {} {{ ... }}", msg.name)),
        });
    }

    for nested in &msg.nested_messages {
        add_message_folding(nested, ranges);
    }

    for nested_enum in &msg.nested_enums {
        if nested_enum.end_line > nested_enum.line {
            ranges.push(FoldingRange {
                start_line: nested_enum.line,
                start_character: None,
                end_line: nested_enum.end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(format!("enum {} {{ ... }}", nested_enum.name)),
            });
        }
    }
}

/// Find the range of contiguous import lines.
fn find_import_range(content: &str) -> Option<(u32, u32)> {
    let mut first_import: Option<u32> = None;
    let mut last_import: Option<u32> = None;

    for (i, line) in content.lines().enumerate() {
        if line.trim().starts_with("import ") {
            let line_num = i as u32;
            if first_import.is_none() {
                first_import = Some(line_num);
            }
            last_import = Some(line_num);
        }
    }

    match (first_import, last_import) {
        (Some(first), Some(last)) if last > first => Some((first, last)),
        _ => None,
    }
}

/// Find multi-line comment ranges (/* ... */).
fn find_comment_ranges(content: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let mut in_block_comment = false;
    let mut comment_start_line = 0u32;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !in_block_comment && trimmed.contains("/*") {
            in_block_comment = true;
            comment_start_line = i as u32;
        }
        if in_block_comment && trimmed.contains("*/") {
            in_block_comment = false;
            let end_line = i as u32;
            if end_line > comment_start_line {
                ranges.push(FoldingRange {
                    start_line: comment_start_line,
                    start_character: None,
                    end_line,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                    collapsed_text: Some("/* ... */".to_string()),
                });
            }
        }
    }

    // Also fold contiguous single-line comments (3+ lines)
    let mut line_comment_start: Option<u32> = None;
    let mut line_comment_end: u32 = 0;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            if line_comment_start.is_none() {
                line_comment_start = Some(i as u32);
            }
            line_comment_end = i as u32;
        } else {
            if let Some(start) = line_comment_start {
                if line_comment_end > start + 1 {
                    ranges.push(FoldingRange {
                        start_line: start,
                        start_character: None,
                        end_line: line_comment_end,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Comment),
                        collapsed_text: Some("// ...".to_string()),
                    });
                }
            }
            line_comment_start = None;
        }
    }
    // Handle trailing comment block
    if let Some(start) = line_comment_start {
        if line_comment_end > start + 1 {
            ranges.push(FoldingRange {
                start_line: start,
                start_character: None,
                end_line: line_comment_end,
                end_character: None,
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: Some("// ...".to_string()),
            });
        }
    }

    ranges
}

/// Find oneof { ... } block ranges by scanning content.
fn find_oneof_ranges(content: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let mut oneof_start: Option<u32> = None;
    let mut brace_depth = 0i32;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("oneof ") && trimmed.contains('{') {
            oneof_start = Some(i as u32);
            brace_depth = 1;
        } else if oneof_start.is_some() {
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            if let Some(start) = oneof_start {
                                ranges.push(FoldingRange {
                                    start_line: start,
                                    start_character: None,
                                    end_line: i as u32,
                                    end_character: None,
                                    kind: Some(FoldingRangeKind::Region),
                                    collapsed_text: Some("oneof { ... }".to_string()),
                                });
                            }
                            oneof_start = None;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_import_range() {
        let content = r#"syntax = "proto3";

import "a.proto";
import "b.proto";
import "c.proto";

message Foo {}
"#;
        let range = find_import_range(content);
        assert_eq!(range, Some((2, 4)));
    }

    #[test]
    fn test_find_comment_ranges() {
        let content = r#"/* This is
a multi-line
comment */
message Foo {}
"#;
        let ranges = find_comment_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 2);
    }

    #[test]
    fn test_find_oneof_ranges() {
        let content = r#"message Foo {
  oneof choice {
    string a = 1;
    int32 b = 2;
  }
}
"#;
        let ranges = find_oneof_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 4);
    }
}
