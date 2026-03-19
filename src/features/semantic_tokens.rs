use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Token types supported by our semantic tokens provider.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::TYPE,           // 0: message names
    SemanticTokenType::ENUM,           // 1: enum names
    SemanticTokenType::ENUM_MEMBER,    // 2: enum values
    SemanticTokenType::INTERFACE,      // 3: service names
    SemanticTokenType::METHOD,         // 4: rpc method names
    SemanticTokenType::PROPERTY,       // 5: field names
    SemanticTokenType::KEYWORD,        // 6: keywords
    SemanticTokenType::NAMESPACE,      // 7: package names
    SemanticTokenType::STRING,         // 8: string literals
    SemanticTokenType::NUMBER,         // 9: number literals
    SemanticTokenType::COMMENT,        // 10: comments
];

/// Token modifiers (none used currently, but required by the protocol).
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
];

const TOKEN_TYPE: u32 = 0;
const TOKEN_ENUM: u32 = 1;
const TOKEN_ENUM_MEMBER: u32 = 2;
const TOKEN_INTERFACE: u32 = 3;
const TOKEN_METHOD: u32 = 4;
const TOKEN_PROPERTY: u32 = 5;
const TOKEN_KEYWORD: u32 = 6;
const TOKEN_NAMESPACE: u32 = 7;
const TOKEN_STRING: u32 = 8;
const TOKEN_NUMBER: u32 = 9;
const TOKEN_COMMENT: u32 = 10;

/// Provide full semantic tokens for a proto file.
pub fn provide_semantic_tokens_full(
    params: SemanticTokensParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<SemanticTokensResult> {
    let uri = &params.text_document.uri;
    let _proto = workspace.get_file(uri)?;
    let content = content?;

    let mut tokens: Vec<RawToken> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num as u32;
        let trimmed = line.trim();

        // Comments
        if trimmed.starts_with("//") {
            let start = line.find("//").unwrap_or(0) as u32;
            tokens.push(RawToken {
                line: line_num,
                start,
                length: (line.len() as u32).saturating_sub(start),
                token_type: TOKEN_COMMENT,
                token_modifiers: 0,
            });
            continue;
        }

        // Block comment start (simplified — single-line only)
        if trimmed.starts_with("/*") {
            let start = line.find("/*").unwrap_or(0) as u32;
            tokens.push(RawToken {
                line: line_num,
                start,
                length: (line.len() as u32).saturating_sub(start),
                token_type: TOKEN_COMMENT,
                token_modifiers: 0,
            });
            continue;
        }

        // Tokenize keywords and other elements on this line
        tokenize_line(line, line_num, trimmed, &mut tokens);
    }

    // Sort tokens by position
    tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start.cmp(&b.start)));

    // Convert to delta-encoded SemanticTokens
    let data = encode_tokens(&tokens);

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

fn tokenize_line(line: &str, line_num: u32, trimmed: &str, tokens: &mut Vec<RawToken>) {
    // Keywords at the beginning of lines
    // Check for keyword-led lines and tokenize accordingly
    if trimmed.starts_with("syntax") {
        add_keyword_token(line, "syntax", line_num, tokens);
        add_string_literals(line, line_num, tokens);
    } else if trimmed.starts_with("package") {
        add_keyword_token(line, "package", line_num, tokens);
        // Package name
        if let Some(name) = extract_after_keyword(trimmed, "package") {
            let name = name.trim_end_matches(';').trim();
            if !name.is_empty() {
                if let Some(pos) = line.find(name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: pos as u32,
                        length: name.len() as u32,
                        token_type: TOKEN_NAMESPACE,
                        token_modifiers: 0,
                    });
                }
            }
        }
    } else if trimmed.starts_with("import") {
        add_keyword_token(line, "import", line_num, tokens);
        add_string_literals(line, line_num, tokens);
    } else if trimmed.starts_with("message") {
        add_keyword_token(line, "message", line_num, tokens);
        if let Some(name) = extract_after_keyword(trimmed, "message") {
            let name = name.trim_end_matches(|c: char| c == '{' || c.is_whitespace());
            if !name.is_empty() {
                if let Some(pos) = line.find(name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: pos as u32,
                        length: name.len() as u32,
                        token_type: TOKEN_TYPE,
                        token_modifiers: 0,
                    });
                }
            }
        }
    } else if trimmed.starts_with("enum") {
        add_keyword_token(line, "enum", line_num, tokens);
        if let Some(name) = extract_after_keyword(trimmed, "enum") {
            let name = name.trim_end_matches(|c: char| c == '{' || c.is_whitespace());
            if !name.is_empty() {
                if let Some(pos) = line.find(name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: pos as u32,
                        length: name.len() as u32,
                        token_type: TOKEN_ENUM,
                        token_modifiers: 0,
                    });
                }
            }
        }
    } else if trimmed.starts_with("service") {
        add_keyword_token(line, "service", line_num, tokens);
        if let Some(name) = extract_after_keyword(trimmed, "service") {
            let name = name.trim_end_matches(|c: char| c == '{' || c.is_whitespace());
            if !name.is_empty() {
                if let Some(pos) = line.find(name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: pos as u32,
                        length: name.len() as u32,
                        token_type: TOKEN_INTERFACE,
                        token_modifiers: 0,
                    });
                }
            }
        }
    } else if trimmed.starts_with("rpc") {
        tokenize_rpc_line(line, line_num, trimmed, tokens);
    } else if trimmed.starts_with("repeated")
        || trimmed.starts_with("optional")
        || trimmed.starts_with("required")
    {
        // Field with label
        let kw = if trimmed.starts_with("repeated") {
            "repeated"
        } else if trimmed.starts_with("optional") {
            "optional"
        } else {
            "required"
        };
        add_keyword_token(line, kw, line_num, tokens);
        tokenize_field_after_label(line, line_num, trimmed, kw, tokens);
    } else if trimmed.starts_with("oneof") {
        add_keyword_token(line, "oneof", line_num, tokens);
    } else if trimmed.starts_with("map") {
        add_keyword_token(line, "map", line_num, tokens);
        tokenize_field_parts(line, line_num, tokens);
    } else if trimmed.starts_with("reserved") {
        add_keyword_token(line, "reserved", line_num, tokens);
        add_number_literals(line, line_num, tokens);
    } else {
        // Could be a field definition (type name = number;) or an enum value (NAME = number;)
        tokenize_field_or_enum_value(line, line_num, trimmed, tokens);
    }
}

fn tokenize_rpc_line(line: &str, line_num: u32, trimmed: &str, tokens: &mut Vec<RawToken>) {
    add_keyword_token(line, "rpc", line_num, tokens);

    // Find method name
    if let Some(after_rpc) = trimmed.strip_prefix("rpc ") {
        let after_rpc = after_rpc.trim_start();
        if let Some(name_end) = after_rpc.find(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            let method_name = &after_rpc[..name_end];
            if !method_name.is_empty() {
                if let Some(pos) = line.find(method_name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: pos as u32,
                        length: method_name.len() as u32,
                        token_type: TOKEN_METHOD,
                        token_modifiers: 0,
                    });
                }
            }
        }
    }

    // Look for 'returns' keyword
    if let Some(pos) = line.find("returns") {
        tokens.push(RawToken {
            line: line_num,
            start: pos as u32,
            length: 7,
            token_type: TOKEN_KEYWORD,
            token_modifiers: 0,
        });
    }

    // Look for 'stream' keywords
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find("stream") {
        let abs_pos = search_start + pos;
        // Verify it's a whole word
        let before_ok = abs_pos == 0
            || !line.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
        let after_ok = abs_pos + 6 >= line.len()
            || !line.as_bytes()[abs_pos + 6].is_ascii_alphanumeric();
        if before_ok && after_ok {
            tokens.push(RawToken {
                line: line_num,
                start: abs_pos as u32,
                length: 6,
                token_type: TOKEN_KEYWORD,
                token_modifiers: 0,
            });
        }
        search_start = abs_pos + 6;
    }

    // Type references in parentheses
    for cap in find_paren_contents(line) {
        let type_name = cap.trim();
        let type_name = type_name.strip_prefix("stream ").unwrap_or(type_name).trim();
        if !type_name.is_empty() {
            if let Some(pos) = line.rfind(type_name) {
                tokens.push(RawToken {
                    line: line_num,
                    start: pos as u32,
                    length: type_name.len() as u32,
                    token_type: TOKEN_TYPE,
                    token_modifiers: 0,
                });
            }
        }
    }
}

fn tokenize_field_after_label(
    line: &str,
    line_num: u32,
    trimmed: &str,
    keyword: &str,
    tokens: &mut Vec<RawToken>,
) {
    if let Some(rest) = trimmed.strip_prefix(keyword) {
        let rest = rest.trim_start();
        // rest should be "type_name field_name = number;"
        tokenize_field_definition(line, line_num, rest, tokens);
    }
}

fn tokenize_field_definition(line: &str, line_num: u32, field_str: &str, tokens: &mut Vec<RawToken>) {
    let parts: Vec<&str> = field_str.split_whitespace().collect();
    if parts.len() >= 2 {
        let type_name = parts[0];
        let field_name = parts[1];

        // Type reference (if not builtin)
        if !is_builtin_type(type_name) && !type_name.starts_with("map") {
            if let Some(pos) = line.find(type_name) {
                tokens.push(RawToken {
                    line: line_num,
                    start: pos as u32,
                    length: type_name.len() as u32,
                    token_type: TOKEN_TYPE,
                    token_modifiers: 0,
                });
            }
        }

        // Field name
        let field_name_clean = field_name.trim_end_matches(|c: char| c == '=' || c.is_whitespace());
        if !field_name_clean.is_empty() {
            // Find the field name position after the type
            if let Some(type_pos) = line.find(type_name) {
                let after_type = type_pos + type_name.len();
                if let Some(rel_pos) = line[after_type..].find(field_name_clean) {
                    let abs_pos = after_type + rel_pos;
                    tokens.push(RawToken {
                        line: line_num,
                        start: abs_pos as u32,
                        length: field_name_clean.len() as u32,
                        token_type: TOKEN_PROPERTY,
                        token_modifiers: 0,
                    });
                }
            }
        }

        // Number literal
        add_number_literals(line, line_num, tokens);
    }
}

fn tokenize_field_or_enum_value(line: &str, line_num: u32, trimmed: &str, tokens: &mut Vec<RawToken>) {
    if trimmed.is_empty() || trimmed == "}" || trimmed == "{" {
        return;
    }

    // Check if it looks like an enum value: NAME = number;
    if trimmed.contains('=') {
        let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim().trim_end_matches(';').trim();

            // If left is all uppercase or starts with uppercase, and right is a number → enum value
            if !left.is_empty() && right.chars().all(|c| c.is_ascii_digit() || c == '-') {
                // Could be an enum value or a field
                // Heuristic: if no space in left (single word), likely enum value or simple field
                if !left.contains(' ') {
                    // Enum value
                    if let Some(pos) = line.find(left) {
                        tokens.push(RawToken {
                            line: line_num,
                            start: pos as u32,
                            length: left.len() as u32,
                            token_type: TOKEN_ENUM_MEMBER,
                            token_modifiers: 0,
                        });
                    }
                    add_number_literals(line, line_num, tokens);
                    return;
                }
            }
        }
    }

    // Field definition: type field_name = number;
    tokenize_field_definition(line, line_num, trimmed, tokens);
}

fn tokenize_field_parts(line: &str, line_num: u32, tokens: &mut Vec<RawToken>) {
    // For map fields, find the field name and number
    if let Some(gt_pos) = line.find('>') {
        let rest = &line[gt_pos + 1..];
        let rest = rest.trim_start();
        if let Some(eq_pos) = rest.find('=') {
            let field_name = rest[..eq_pos].trim();
            if !field_name.is_empty() {
                let abs_pos = gt_pos + 1 + (rest.len() - rest.trim_start().len());
                if let Some(name_pos) = line[abs_pos..].find(field_name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: (abs_pos + name_pos) as u32,
                        length: field_name.len() as u32,
                        token_type: TOKEN_PROPERTY,
                        token_modifiers: 0,
                    });
                }
            }
        }
    }
    add_number_literals(line, line_num, tokens);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn add_keyword_token(line: &str, keyword: &str, line_num: u32, tokens: &mut Vec<RawToken>) {
    if let Some(pos) = line.find(keyword) {
        tokens.push(RawToken {
            line: line_num,
            start: pos as u32,
            length: keyword.len() as u32,
            token_type: TOKEN_KEYWORD,
            token_modifiers: 0,
        });
    }
}

fn add_string_literals(line: &str, line_num: u32, tokens: &mut Vec<RawToken>) {
    let mut in_string = false;
    let mut string_start = 0;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            if in_string {
                tokens.push(RawToken {
                    line: line_num,
                    start: string_start as u32,
                    length: (i - string_start + 1) as u32,
                    token_type: TOKEN_STRING,
                    token_modifiers: 0,
                });
                in_string = false;
            } else {
                string_start = i;
                in_string = true;
            }
        }
    }
}

fn add_number_literals(line: &str, line_num: u32, tokens: &mut Vec<RawToken>) {
    // Find number after '='
    if let Some(eq_pos) = line.find('=') {
        let after_eq = &line[eq_pos + 1..];
        let trimmed = after_eq.trim_start();
        let offset = after_eq.len() - trimmed.len();
        let num_len = trimmed
            .find(|c: char| !c.is_ascii_digit() && c != '-')
            .unwrap_or(trimmed.len());
        if num_len > 0 {
            let abs_start = eq_pos + 1 + offset;
            tokens.push(RawToken {
                line: line_num,
                start: abs_start as u32,
                length: num_len as u32,
                token_type: TOKEN_NUMBER,
                token_modifiers: 0,
            });
        }
    }
}

fn extract_after_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    line.strip_prefix(keyword).map(|s| s.trim_start())
}

fn find_paren_contents(line: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut search_from = 0;
    while let Some(open) = line[search_from..].find('(') {
        let abs_open = search_from + open;
        if let Some(close) = line[abs_open..].find(')') {
            let content = &line[abs_open + 1..abs_open + close];
            results.push(content);
            search_from = abs_open + close + 1;
        } else {
            break;
        }
    }
    results
}

fn is_builtin_type(t: &str) -> bool {
    matches!(
        t,
        "double"
            | "float"
            | "int32"
            | "int64"
            | "uint32"
            | "uint64"
            | "sint32"
            | "sint64"
            | "fixed32"
            | "fixed64"
            | "sfixed32"
            | "sfixed64"
            | "bool"
            | "string"
            | "bytes"
    )
}

fn encode_tokens(raw: &[RawToken]) -> Vec<SemanticToken> {
    let mut result = Vec::with_capacity(raw.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in raw {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers,
        });

        prev_line = token.line;
        prev_start = token.start;
    }

    result
}

/// Build the `SemanticTokensLegend` used in capabilities.
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_tokens() {
        let raw = vec![
            RawToken { line: 0, start: 0, length: 6, token_type: TOKEN_KEYWORD, token_modifiers: 0 },
            RawToken { line: 0, start: 9, length: 8, token_type: TOKEN_STRING, token_modifiers: 0 },
            RawToken { line: 2, start: 0, length: 7, token_type: TOKEN_KEYWORD, token_modifiers: 0 },
        ];
        let encoded = encode_tokens(&raw);
        assert_eq!(encoded.len(), 3);
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].delta_start, 0);
        assert_eq!(encoded[1].delta_line, 0);
        assert_eq!(encoded[1].delta_start, 9);
        assert_eq!(encoded[2].delta_line, 2);
        assert_eq!(encoded[2].delta_start, 0);
    }

    #[test]
    fn test_find_paren_contents() {
        let result = find_paren_contents("rpc GetUser(GetUserRequest) returns (GetUserResponse);");
        assert_eq!(result, vec!["GetUserRequest", "GetUserResponse"]);
    }

    #[test]
    fn test_is_builtin_type() {
        assert!(is_builtin_type("string"));
        assert!(is_builtin_type("int32"));
        assert!(!is_builtin_type("UserMessage"));
    }
}
