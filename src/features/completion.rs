use crate::parser::ParsedProto;
use crate::workspace::{WorkspaceManager, SymbolKind};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Documentation,
    MarkupContent, MarkupKind, Position, Url,
};

const PROTO_KEYWORDS: &[&str] = &[
    "syntax",
    "package",
    "import",
    "option",
    "message",
    "enum",
    "service",
    "rpc",
    "returns",
    "repeated",
    "optional",
    "required",
    "reserved",
    "extend",
    "oneof",
    "map",
];

const PROTO_TYPES: &[&str] = &[
    "double", "float", "int32", "int64", "uint32", "uint64", "sint32", "sint64", "fixed32",
    "fixed64", "sfixed32", "sfixed64", "bool", "string", "bytes",
];

pub async fn provide_completion(
    params: CompletionParams,
    workspace: &WorkspaceManager,
    document_content: Option<&str>,
) -> Option<CompletionResponse> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let proto = workspace.get_file(&uri)?;

    // Get context from cursor position
    let context = document_content.map(|content| get_completion_context(content, position, &proto))?;

    
    // Add completion items based on context
    let mut items = Vec::new();

    // Add items with appropriate priority based on context
    add_contextual_completions(&context, &proto, workspace, &uri, &mut items).await;

    
    // Sort items by priority (lower sort_text = higher priority)
    items.sort_by(|a, b| a.sort_text.as_ref().unwrap_or(&"0".to_string()).cmp(b.sort_text.as_ref().unwrap_or(&"0".to_string())));

    Some(CompletionResponse::Array(items))
}

/// Represents the context at the cursor position
#[derive(Debug, Clone)]
struct CompletionContext {
    /// Current line text
    current_line: String,
    /// Text before cursor on current line
    prefix: String,
    /// Whether we're inside a message definition
    in_message: bool,
    /// Whether we're inside an enum definition
    in_enum: bool,
    /// Whether we're inside a service definition
    in_service: bool,
    /// The current package (if any)
    current_package: Option<String>,
    /// whether we're at top level (not inside any block)
    at_top_level: bool,
    /// Package prefix being typed (e.g., "mmsearch." when typing "mmsearch.")
    package_prefix: Option<String>,
    /// Whether we're typing a package name (without dot)
    typing_package_name: bool,
    /// The partial package name being typed
    partial_package: Option<String>,
}

/// Gets the completion context based on cursor position
fn get_completion_context(content: &str, position: Position, proto: &ParsedProto) -> CompletionContext {
    let lines: Vec<&str> = content.lines().collect();
    let line_index = position.line as usize;

    let current_line = if line_index < lines.len() {
        lines[line_index].to_string()
    } else {
        String::new()
    };

    let char_index = position.character as usize;
    let prefix = if char_index <= current_line.len() {
        current_line[..char_index].to_string()
    } else {
        current_line.clone()
    };

    // Check if we're inside various blocks by looking at previous lines
    let mut in_message = false;
    let mut in_enum = false;
    let mut in_service = false;
    let mut brace_count = 0;

    for i in 0..=line_index {
        let line = if i < lines.len() { lines[i] } else { "" };

        // Count braces to determine nesting level
        for ch in line.chars() {
            if ch == '{' {
                brace_count += 1;
            } else if ch == '}' {
                brace_count -= 1;
            }
        }

        // Check for block starts
        if line.trim().starts_with("message ") && i < line_index {
            in_message = true;
            in_enum = false;
            in_service = false;
        } else if line.trim().starts_with("enum ") && i < line_index {
            in_enum = true;
            in_message = false;
            in_service = false;
        } else if line.trim().starts_with("service ") && i < line_index {
            in_service = true;
            in_message = false;
            in_enum = false;
        }
    }

    let at_top_level = brace_count == 0;

    // Extract the identifier before cursor
    let mut identifier_start = char_index;
    while identifier_start > 0 {
        let ch = current_line.chars().nth(identifier_start - 1).unwrap_or(' ');
        if ch.is_alphanumeric() || ch == '_' || ch == '.' {
            identifier_start -= 1;
        } else {
            break;
        }
    }
    let identifier = if identifier_start < char_index {
        &current_line[identifier_start..char_index]
    } else {
        ""
    };

    // Analyze the identifier to determine context
    let (package_prefix, typing_package_name, partial_package) = if identifier.contains('.') {
        // Has dots - check if it ends with a dot (package prefix)
        if identifier.ends_with('.') {
            let pkg_name = &identifier[..identifier.len() - 1];
            if pkg_name.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_') {
                (Some(identifier.to_string()), false, None)
            } else {
                (None, false, None)
            }
        } else {
            // Has dots but doesn't end with dot - might be package.symbol
            if let Some(last_dot) = identifier.rfind('.') {
                let _after_dot = &identifier[last_dot + 1..];
                let before_dot = &identifier[..last_dot];

                if before_dot.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_') {
                    // This is package.partial_symbol
                    (Some(format!("{}.", before_dot)), false, None)
                } else {
                    (None, false, None)
                }
            } else {
                (None, false, None)
            }
        }
    } else {
        // No dots - check if it looks like a package name
        // Only consider it a package name if we're at top level or in specific contexts
        let is_package_context = at_top_level || (identifier.len() > 1 && !in_message && !in_enum && !in_service);
        if identifier.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_') && !identifier.is_empty() && is_package_context {
            (None, true, Some(identifier.to_string()))
        } else {
            (None, false, None)
        }
    };

    CompletionContext {
        current_line,
        prefix,
        in_message,
        in_enum,
        in_service,
        current_package: proto.package.clone(),
        at_top_level,
        package_prefix,
        typing_package_name,
        partial_package,
    }
}

/// Adds completion items based on context
async fn add_contextual_completions(
    context: &CompletionContext,
    proto: &ParsedProto,
    workspace: &WorkspaceManager,
    uri: &Url,
    items: &mut Vec<CompletionItem>,
) {
    // If we're typing a package name (without dot), suggest available packages
    if context.typing_package_name {
        if let Some(partial) = &context.partial_package {
            let symbols_by_package = workspace.get_symbols_by_package_async(uri).await;

            // Show all packages that start with the partial input
            let matching_packages: Vec<_> = symbols_by_package
                .keys()
                .filter(|pkg| pkg.starts_with(partial))
                .collect();

            // If the partial exactly matches a package, also show it with a dot
            if symbols_by_package.contains_key(partial) {
                items.push(CompletionItem {
                    label: format!("{}.", partial),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("Package: {}", partial)),
                    sort_text: Some("00".to_string()), // Highest priority
                    insert_text: Some(format!("{}.", partial)),
                    ..Default::default()
                });
            }

            // Show other matching packages
            for package_name in matching_packages {
                if package_name != partial {
                    items.push(CompletionItem {
                        label: format!("{}.", package_name),
                        kind: Some(CompletionItemKind::MODULE),
                        detail: Some(format!("Package: {}", package_name)),
                        sort_text: Some(format!("0{}", package_name)),
                        insert_text: Some(format!("{}.", package_name)),
                        ..Default::default()
                    });
                }
            }
        } else {
            // No partial input, show all available packages
            let symbols_by_package = workspace.get_symbols_by_package_async(uri).await;
            let mut packages: Vec<_> = symbols_by_package.keys().collect();
            packages.sort();

            for package_name in packages {
                items.push(CompletionItem {
                    label: format!("{}.", package_name),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("Package: {}", package_name)),
                    sort_text: Some(format!("0{}", package_name)),
                    insert_text: Some(format!("{}.", package_name)),
                    ..Default::default()
                });
            }
        }
        return;
    }

    // If we have a package prefix (e.g., "mmsearch."), show symbols from that package
    if let Some(pkg_prefix) = &context.package_prefix {
        let pkg_name = &pkg_prefix[..pkg_prefix.len() - 1]; // Remove the trailing dot
        tracing::debug!("Package prefix detected: '{}', looking for package: '{}'", pkg_prefix, pkg_name);

        let symbols_by_package = workspace.get_symbols_by_package_async(uri).await;

        tracing::debug!("Available packages: {:?}", symbols_by_package.keys().collect::<Vec<_>>());

        if let Some(symbols) = symbols_by_package.get(pkg_name) {
            tracing::debug!("Found {} symbols in package '{}'", symbols.len(), pkg_name);
            for symbol in symbols {
                let kind = match symbol.kind {
                    SymbolKind::Message => CompletionItemKind::CLASS,
                    SymbolKind::Enum => CompletionItemKind::ENUM,
                    SymbolKind::EnumValue => CompletionItemKind::ENUM_MEMBER,
                    SymbolKind::Service => CompletionItemKind::INTERFACE,
                    SymbolKind::Method => CompletionItemKind::METHOD,
                };

                items.push(CompletionItem {
                    label: symbol.name.clone(),
                    kind: Some(kind),
                    detail: Some(format!("{}: {}", format!("{:?}", symbol.kind).to_lowercase(), symbol.full_name)),
                    sort_text: Some(format!("0{}", symbol.name)), // High priority for package symbols
                    ..Default::default()
                });
            }
        } else {
            tracing::debug!("No symbols found for package '{}'", pkg_name);
        }
        return;
    }

    // Determine priority based on context
    let priority_base = if context.at_top_level {
        "0" // Highest priority for top-level
    } else if context.in_message {
        "1" // High priority inside message
    } else if context.in_service {
        "2" // Medium priority inside service
    } else if context.in_enum {
        "3" // Lower priority inside enum
    } else {
        "4" // Lowest priority for other contexts
    };

    // At top level, suggest top-level declarations
    if context.at_top_level {
        // Top-level keywords with highest priority
        for keyword in ["syntax", "package", "import", "option", "message", "enum", "service", "extend"] {
            if !PROTO_KEYWORDS.contains(&keyword) {
                continue;
            }

            let mut sort_text = format!("{}{}", priority_base, keyword);
            // Give extra priority to package if not already declared
            if keyword == "package" && context.current_package.is_none() {
                sort_text = format!("00{}", keyword); // Highest priority
            }
            // Give slightly lower priority to extend at top level
            else if keyword == "extend" {
                sort_text = format!("1{}", keyword); // Lower than main keywords but still available
            }

            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Protobuf keyword".to_string()),
                sort_text: Some(sort_text),
                filter_text: Some(keyword.to_string()),
                ..Default::default()
            });
        }
    }

    // Inside message, suggest field-related keywords and types
    if context.in_message {
        // Field labels
        for label in ["optional", "required", "repeated"] {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Field label".to_string()),
                sort_text: Some(format!("{}{}", priority_base, label)),
                filter_text: Some(label.to_string()),
                ..Default::default()
            });
        }

        // Built-in types with high priority
        for proto_type in PROTO_TYPES {
            items.push(CompletionItem {
                label: proto_type.to_string(),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                detail: Some("Built-in type".to_string()),
                sort_text: Some(format!("{}{}", priority_base, proto_type)),
                filter_text: Some(proto_type.to_string()),
                ..Default::default()
            });
        }

        // Message-specific keywords
        for keyword in ["oneof", "map", "option", "reserved", "extend"] {
            let mut priority = priority_base.to_string();
            // Give lower priority to extend inside messages
            if keyword == "extend" {
                priority = format!("{}{}", priority_base, "9");
            }

            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Message keyword".to_string()),
                sort_text: Some(format!("{}{}", priority, keyword)),
                filter_text: Some(keyword.to_string()),
                ..Default::default()
            });
        }
    }

    // Inside service, suggest RPC-related keywords
    if context.in_service {
        for keyword in ["rpc", "option", "returns"] {
            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Service keyword".to_string()),
                sort_text: Some(format!("{}{}", priority_base, keyword)),
                filter_text: Some(keyword.to_string()),
                ..Default::default()
            });
        }
    }

    // Inside enum, suggest enum-specific keywords
    if context.in_enum {
        for keyword in ["option", "reserved"] {
            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Enum keyword".to_string()),
                sort_text: Some(format!("{}{}", priority_base, keyword)),
                filter_text: Some(keyword.to_string()),
                ..Default::default()
            });
        }
    }

    // Add messages with priority based on package context
    add_messages_with_priority(&proto, items, context, priority_base);

    // Add enums with priority
    add_enums_with_priority(&proto, items, context, priority_base);

    // Add services with priority
    add_services_with_priority(&proto, items, context, priority_base);

    // Add items from imported files with lower priority
    for import in &proto.imports {
        if let Some(imported) = workspace.get_imported_file_cached(uri, &import.path) {
            add_messages_with_priority(&imported, items, context, "5"); // Lowest priority
            add_enums_with_priority(&imported, items, context, "5");
            add_services_with_priority(&imported, items, context, "5");
        }
    }

    // Add remaining keywords with lowest priority (except extend which gets medium-low priority)
    for keyword in PROTO_KEYWORDS {
        // Skip if already added based on context
        if items.iter().any(|item| item.label == *keyword) {
            continue;
        }

        // Give slightly higher priority to extend as it's a useful feature
        let priority = if *keyword == "extend" {
            "6" // Medium-low priority for extend
        } else if *keyword == "optional" || *keyword == "required" || *keyword == "repeated" {
            "7" // Low priority for field labels (useful outside messages)
        } else {
            "9" // Lowest priority for other unused keywords
        };

        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Protobuf keyword".to_string()),
            sort_text: Some(format!("{}{}", priority, keyword)),
            filter_text: Some(keyword.to_string()),
            ..Default::default()
        });
    }

    // Also add built-in types with low priority if not in message context
    if !context.in_message {
        for proto_type in PROTO_TYPES {
            // Skip if already added
            if items.iter().any(|item| item.label == *proto_type) {
                continue;
            }

            items.push(CompletionItem {
                label: proto_type.to_string(),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                detail: Some("Built-in type".to_string()),
                sort_text: Some(format!("8{}", proto_type)), // Low priority for types outside messages
                filter_text: Some(proto_type.to_string()),
                ..Default::default()
            });
        }
    }
}

/// Adds messages to completion with appropriate priority
fn add_messages_with_priority(proto: &ParsedProto, items: &mut Vec<CompletionItem>, context: &CompletionContext, priority_base: &str) {
    for msg in &proto.messages {
        // Higher priority for messages in the same package
        let priority = if let (Some(current_pkg), Some(msg_pkg)) = (&context.current_package, msg.full_name.split('.').nth(0)) {
            if current_pkg == msg_pkg {
                format!("{}{}", priority_base, "0")
            } else {
                format!("{}{}", priority_base, "1")
            }
        } else {
            format!("{}{}", priority_base, "2")
        };

        items.push(CompletionItem {
            label: msg.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(format!("Message: {}", msg.full_name)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```protobuf\nmessage {}\n```", msg.name),
            })),
            sort_text: Some(priority),
            ..Default::default()
        });

        // Add nested messages
        add_nested_messages_with_priority(msg, items, context, &format!("{}{}", priority_base, "1"));
    }
}

/// Adds nested messages with priority
fn add_nested_messages_with_priority(
    msg: &crate::parser::proto::MessageElement,
    items: &mut Vec<CompletionItem>,
    context: &CompletionContext,
    priority_base: &str,
) {
    for nested in &msg.nested_messages {
        items.push(CompletionItem {
            label: nested.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(format!("Nested message: {}", nested.full_name)),
            sort_text: Some(format!("{}{}", priority_base, "1")),
            ..Default::default()
        });
        add_nested_messages_with_priority(nested, items, context, priority_base);
    }
}

/// Adds enums to completion with appropriate priority
fn add_enums_with_priority(proto: &ParsedProto, items: &mut Vec<CompletionItem>, context: &CompletionContext, priority_base: &str) {
    for e in &proto.enums {
        // Higher priority for enums in the same package
        let priority = if let (Some(current_pkg), Some(enum_pkg)) = (&context.current_package, e.full_name.split('.').nth(0)) {
            if current_pkg == enum_pkg {
                format!("{}{}", priority_base, "0")
            } else {
                format!("{}{}", priority_base, "1")
            }
        } else {
            format!("{}{}", priority_base, "2")
        };

        items.push(CompletionItem {
            label: e.name.clone(),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some(format!("Enum: {}", e.full_name)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```protobuf\nenum {}\n```", e.name),
            })),
            sort_text: Some(priority),
            ..Default::default()
        });

        // Add enum values
        for value in &e.values {
            items.push(CompletionItem {
                label: value.name.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(format!("Enum value: {} = {}", value.name, value.number)),
                sort_text: Some(format!("{}{}", priority_base, "2")),
                ..Default::default()
            });
        }
    }
}

/// Adds services to completion with appropriate priority
fn add_services_with_priority(proto: &ParsedProto, items: &mut Vec<CompletionItem>, context: &CompletionContext, priority_base: &str) {
    for svc in &proto.services {
        // Higher priority for services in the same package
        let priority = if let (Some(current_pkg), Some(svc_pkg)) = (&context.current_package, svc.full_name.split('.').nth(0)) {
            if current_pkg == svc_pkg {
                format!("{}{}", priority_base, "0")
            } else {
                format!("{}{}", priority_base, "1")
            }
        } else {
            format!("{}{}", priority_base, "2")
        };

        items.push(CompletionItem {
            label: svc.name.clone(),
            kind: Some(CompletionItemKind::INTERFACE),
            detail: Some(format!("Service: {}", svc.full_name)),
            sort_text: Some(priority),
            ..Default::default()
        });

        // Add methods
        for method in &svc.methods {
            items.push(CompletionItem {
                label: method.name.clone(),
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(format!(
                    "rpc {}({}) returns ({})",
                    method.name, method.input_type, method.output_type
                )),
                sort_text: Some(format!("{}{}", priority_base, "1")),
                ..Default::default()
            });
        }
    }
}