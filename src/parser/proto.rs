use anyhow::Result;
use protobuf::descriptor::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Position;

/// Import element with line number information
#[derive(Debug, Clone)]
pub struct ImportElement {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// Parse error with location information
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
    pub character: u32,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone)]
pub enum ErrorSeverity {
    Error,
    Warning,
    Info,
}

/// Parsed protobuf file with all its elements
#[derive(Debug, Clone)]
pub struct ParsedProto {
    pub uri: String,
    pub package: Option<String>,
    pub imports: Vec<ImportElement>,
    pub messages: Vec<MessageElement>,
    pub enums: Vec<EnumElement>,
    pub services: Vec<ServiceElement>,
    pub line_to_element: HashMap<u32, ProtoElement>,
    /// Parse errors collected during parsing
    pub parse_errors: Vec<ParseError>,
    /// File descriptor for advanced operations
    pub file_descriptor: Option<FileDescriptorProto>,
}

/// Message definition element
#[derive(Debug, Clone)]
pub struct MessageElement {
    pub name: String,
    pub full_name: String,
    pub fields: Vec<FieldElement>,
    pub nested_messages: Vec<MessageElement>,
    pub nested_enums: Vec<EnumElement>,
    pub line: u32,
    pub end_line: u32,
    pub character: u32,
}

/// Field definition element
#[derive(Debug, Clone)]
pub struct FieldElement {
    pub name: String,
    pub field_type: String,
    pub type_name: Option<String>,
    pub number: i32,
    pub label: Option<FieldLabelProto>,
    pub line: u32,
    pub character: u32,
}

/// Enum definition element
#[derive(Debug, Clone)]
pub struct EnumElement {
    pub name: String,
    pub full_name: String,
    pub values: Vec<EnumValueElement>,
    pub line: u32,
    pub end_line: u32,
    pub character: u32,
}

/// Enum value element
#[derive(Debug, Clone)]
pub struct EnumValueElement {
    pub name: String,
    pub number: i32,
    pub line: u32,
    pub character: u32,
}

/// Service definition element
#[derive(Debug, Clone)]
pub struct ServiceElement {
    pub name: String,
    pub full_name: String,
    pub methods: Vec<MethodElement>,
    pub line: u32,
    pub end_line: u32,
    pub character: u32,
}

/// RPC method element
#[derive(Debug, Clone)]
pub struct MethodElement {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub line: u32,
    pub character: u32,
}

/// Field label (optional, required, repeated)
#[derive(Debug, Clone)]
pub enum FieldLabelProto {
    Optional,
    Required,
    Repeated,
}

/// Protobuf element type
#[derive(Debug, Clone)]
pub enum ProtoElement {
    Message(MessageElement),
    Enum(EnumElement),
    Service(ServiceElement),
    Field(FieldElement),
    Method(MethodElement),
}

/// Parser for protobuf files using protobuf-parse library
pub struct ProtoParser {
    cache: Arc<RwLock<HashMap<String, ParsedProto>>>,
}

impl ProtoParser {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse a protobuf file from content
    pub async fn parse(&self, uri: String, content: &str) -> Result<ParsedProto> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&uri) {
                return Ok(cached.clone());
            }
        }

        // Create temporary file for parsing
        let temp_dir = tempfile::tempdir()?;
        let file_path = temp_dir.path().join("temp.proto");
        std::fs::write(&file_path, content)?;

        // Force use of simple parser for now to ensure line numbers are correct
        // TODO: Fix convert_file_descriptor line number handling and re-enable protobuf-parse
        let (parse_result, parse_errors) = (self.parse_simple(&uri, content)?, Vec::<ParseError>::new());

        /*
        // Parse using protobuf-parse
        let (parse_result, parse_errors) = match Parser::new()
            .pure()
            .include(temp_dir.path())
            .input(&file_path)
            .parse_and_typecheck()
        {
            Ok(parsed) => {
                // Find our file descriptor
                if let Some(fd) = parsed.file_descriptors
                    .iter()
                    .find(|fd| fd.name() == "temp.proto")
                    .cloned()
                {
                    // Check if service count is reasonable (protobuf-parse might drop services with custom options)
                    if fd.service.is_empty() && content.contains("service") {
                        // Fallback to simple parsing if services were dropped
                        (self.parse_simple(&uri, content)?, Vec::new())
                    } else {
                        (self.convert_file_descriptor(&uri, &fd, &parsed.file_descriptors)?, Vec::new())
                    }
                } else {
                    // Fallback to simple parsing
                    (self.parse_simple(&uri, content)?, Vec::new())
                }
            }
            Err(e) => {
                // Extract errors from protobuf-parse
                let errors = self.extract_protobuf_parse_errors(&e, content.lines().count() as u32);

                // Check if we have useful error information from protobuf-parse
                let has_useful_errors = errors.iter().any(|e| e.line > 0 || e.character > 0);

                if has_useful_errors {
                    // Use empty structure with just the errors from protobuf-parse
                    let empty_result = ParsedProto {
                        uri: uri.clone(),
                        package: None,
                        imports: Vec::new(),
                        messages: Vec::new(),
                        enums: Vec::new(),
                        services: Vec::new(),
                        line_to_element: HashMap::new(),
                        parse_errors: errors.clone(),
                        file_descriptor: None,
                    };
                    (empty_result, errors)
                } else {
                    // Try to get partial results with simple parsing
                    let simple_result = self.parse_simple(&uri, content).unwrap_or_else(|_| ParsedProto {
                        uri: uri.clone(),
                        package: None,
                        imports: Vec::new(),
                        messages: Vec::new(),
                        enums: Vec::new(),
                        services: Vec::new(),
                        line_to_element: HashMap::new(),
                        parse_errors: errors.clone(),
                        file_descriptor: None,
                    });
                    (simple_result, errors)
                }
            }
        };
        */

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(uri.clone(), parse_result.clone());
        }

        // Parse result already includes errors, no need to merge
        Ok(parse_result)
    }

    /// Convert FileDescriptorProto to our ParsedProto representation
    fn convert_file_descriptor(
        &self,
        uri: &str,
        fd: &FileDescriptorProto,
        all_fds: &[FileDescriptorProto],
    ) -> Result<ParsedProto> {
        let package = fd.package.clone();
        // Convert dependencies to ImportElements (without line numbers from protobuf-parse)
        let imports: Vec<ImportElement> = fd.dependency
            .iter()
            .enumerate()
            .map(|(idx, path)| ImportElement {
                path: path.clone(),
                line: idx as u32, // Use index as placeholder line number
                character: 0,
            })
            .collect();
        let mut messages = Vec::new();
        let mut enums = Vec::new();
        let mut services = Vec::new();
        let mut line_to_element = HashMap::new();

        // Convert messages
        for (idx, msg_desc) in fd.message_type.iter().enumerate() {
            let msg = self.convert_message(msg_desc, &package, "", 0)?;
            line_to_element.insert(msg.line, ProtoElement::Message(msg.clone()));
            messages.push(msg);
        }

        // Convert enums
        for (idx, enum_desc) in fd.enum_type.iter().enumerate() {
            let enum_elem = self.convert_enum(enum_desc, &package, "", 0)?;
            line_to_element.insert(enum_elem.line, ProtoElement::Enum(enum_elem.clone()));
            enums.push(enum_elem);
        }

        // Convert services
        for (idx, service_desc) in fd.service.iter().enumerate() {
            let service = self.convert_service(service_desc, &package, 0)?;
            line_to_element.insert(service.line, ProtoElement::Service(service.clone()));
            services.push(service);
        }

        Ok(ParsedProto {
            uri: uri.to_string(),
            package,
            imports,
            messages,
            enums,
            services,
            line_to_element,
            parse_errors: Vec::new(), // No parse errors when using protobuf-parse
            file_descriptor: Some(fd.clone()),
        })
    }

    /// Convert DescriptorProto to MessageElement
    fn convert_message(
        &self,
        msg: &DescriptorProto,
        package: &Option<String>,
        parent_name: &str,
        base_line: u32,
    ) -> Result<MessageElement> {
        let name = msg.name.clone().unwrap_or_default();
        let full_name = if let Some(pkg) = package {
            if parent_name.is_empty() {
                format!("{}.{}", pkg, name)
            } else {
                format!("{}.{}.{}", pkg, parent_name, name)
            }
        } else {
            if parent_name.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", parent_name, name)
            }
        };

        let mut fields = Vec::new();
        let mut nested_messages = Vec::new();
        let mut nested_enums = Vec::new();

        // Convert fields
        for (idx, field) in msg.field.iter().enumerate() {
            let field_elem = FieldElement {
                name: field.name.clone().unwrap_or_default(),
                field_type: self.field_type_to_string(field.type_.map(|t| t.value())),
                type_name: field.type_name.clone(),
                number: field.number.unwrap_or(0) as i32,
                label: field.label.map(|l| match l.value() {
                    1 => FieldLabelProto::Optional,
                    2 => FieldLabelProto::Required,
                    3 => FieldLabelProto::Repeated,
                    _ => FieldLabelProto::Optional,
                }),
                line: base_line + idx as u32,
                character: 0,
            };
            fields.push(field_elem);
        }

        // Convert nested messages
        for nested_msg in &msg.nested_type {
            let nested = self.convert_message(nested_msg, package, &full_name, base_line + 10)?;
            nested_messages.push(nested);
        }

        // Convert nested enums
        for nested_enum in &msg.enum_type {
            let nested = self.convert_enum(nested_enum, package, &full_name, base_line)?;
            nested_enums.push(nested);
        }

        Ok(MessageElement {
            name,
            full_name,
            fields,
            nested_messages,
            nested_enums,
            line: base_line,
            end_line: base_line + 10,
            character: 0,
        })
    }

    /// Convert EnumDescriptorProto to EnumElement
    fn convert_enum(
        &self,
        enum_desc: &EnumDescriptorProto,
        package: &Option<String>,
        parent_name: &str,
        base_line: u32,
    ) -> Result<EnumElement> {
        let name = enum_desc.name.clone().unwrap_or_default();
        let full_name = if let Some(pkg) = package {
            if parent_name.is_empty() {
                format!("{}.{}", pkg, name)
            } else {
                format!("{}.{}.{}", pkg, parent_name, name)
            }
        } else {
            if parent_name.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", parent_name, name)
            }
        };

        let mut values = Vec::new();
        for (idx, value) in enum_desc.value.iter().enumerate() {
            values.push(EnumValueElement {
                name: value.name.clone().unwrap_or_default(),
                number: value.number.unwrap_or(0) as i32,
                line: base_line + idx as u32 + 1,
                character: 4,
            });
        }

        let values_len = values.len();
        Ok(EnumElement {
            name,
            full_name,
            values,
            line: base_line,
            end_line: base_line + values_len as u32 + 1,
            character: 0,
        })
    }

    /// Convert ServiceDescriptorProto to ServiceElement
    fn convert_service(
        &self,
        service_desc: &ServiceDescriptorProto,
        package: &Option<String>,
        base_line: u32,
    ) -> Result<ServiceElement> {
        let name = service_desc.name.clone().unwrap_or_default();
        let full_name = if let Some(pkg) = package {
            format!("{}.{}", pkg, name)
        } else {
            name.clone()
        };

        let mut methods = Vec::new();
        for (idx, method) in service_desc.method.iter().enumerate() {
            methods.push(MethodElement {
                name: method.name.clone().unwrap_or_default(),
                input_type: method.input_type.clone().unwrap_or_default(),
                output_type: method.output_type.clone().unwrap_or_default(),
                client_streaming: method.client_streaming.unwrap_or(false),
                server_streaming: method.server_streaming.unwrap_or(false),
                line: base_line + idx as u32 + 1,
                character: 4,
            });
        }

        let methods_len = methods.len();
        Ok(ServiceElement {
            name,
            full_name,
            methods,
            line: base_line,
            end_line: base_line + methods_len as u32 + 1,
            character: 0,
        })
    }

    /// Convert field type enum to string
    fn field_type_to_string(&self, field_type: Option<i32>) -> String {
        match field_type {
            Some(1) => "double".to_string(),
            Some(2) => "float".to_string(),
            Some(3) => "int64".to_string(),
            Some(4) => "uint64".to_string(),
            Some(5) => "int32".to_string(),
            Some(6) => "fixed64".to_string(),
            Some(7) => "fixed32".to_string(),
            Some(8) => "bool".to_string(),
            Some(9) => "string".to_string(),
            Some(10) => "group".to_string(),
            Some(11) => "message".to_string(),
            Some(12) => "bytes".to_string(),
            Some(13) => "uint32".to_string(),
            Some(14) => "enum".to_string(),
            Some(15) => "sfixed32".to_string(),
            Some(16) => "sfixed64".to_string(),
            Some(17) => "sint32".to_string(),
            Some(18) => "sint64".to_string(),
            None => "unknown".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Fallback simple parser (for when protobuf-parse fails)
    pub fn parse_simple(&self, uri: &str, content: &str) -> Result<ParsedProto> {
        let mut package = None;
        let mut imports = Vec::new();
        let mut messages = Vec::new();
        let mut enums = Vec::new();
        let mut services = Vec::new();
        let mut line_to_element = HashMap::new();
        let mut parse_errors = Vec::new();

        let mut current_line = 0u32;
        let mut message_stack: Vec<(String, u32, Vec<FieldElement>, Vec<MessageElement>, Vec<EnumElement>)> = Vec::new();
        let mut enum_stack: Vec<(String, u32, Vec<EnumValueElement>)> = Vec::new();
        let mut is_proto3 = false; // Track syntax version
        let mut multiline_field: Option<(String, String, u32)> = None; // (field_name, field_type, start_line)
        let mut in_custom_option = false; // Track if we're inside a custom option block
        let mut custom_option_brace_count = 0; // Track nesting level in custom options
        let mut in_block_comment = false; // Track if we're inside a /* */ block comment

        for (line_idx, line) in content.lines().enumerate() {
            let line_number = line_idx as u32;
            let trimmed = line.trim();

            // First check for line comments (//) - they take precedence over block comments
            if trimmed.starts_with("//") {
                continue; // Skip the entire line comment
            }

            // Handle block comment detection and stripping
            let processed_line = if in_block_comment {
                // We're inside a block comment, look for the end
                if let Some(end_pos) = line.find("*/") {
                    in_block_comment = false;
                    // Return the part after the block comment ends
                    line[end_pos + 2..].to_string()
                } else {
                    // Still inside block comment, return empty string to skip
                    String::new()
                }
            } else {
                // Not in a block comment, check if this line starts one
                if let Some(start_pos) = line.find("/*") {
                    if let Some(end_pos) = line[start_pos..].find("*/") {
                        // Block comment starts and ends on same line
                        // Remove the comment from the line
                        let comment_end = start_pos + end_pos + 2;
                        format!("{}{}",
                            &line[..start_pos],
                            &line[comment_end..])
                    } else {
                        // Block comment starts here and continues
                        in_block_comment = true;
                        // Return the part before the comment
                        line[..start_pos].to_string()
                    }
                } else {
                    // No block comment, return the line as-is
                    line.to_string()
                }
            };

            let trimmed = processed_line.trim();

            // Skip empty lines after comment processing
            if trimmed.is_empty() {
                continue;
            }

            // Check for syntax declaration
            if trimmed.starts_with("syntax ") {
                if trimmed.contains("\"proto3\"") {
                    is_proto3 = true;
                } else if trimmed.contains("\"proto2\"") {
                    is_proto3 = false;
                }
            }

            // Track custom option blocks
            if trimmed.contains('[') && trimmed.contains('(') {
                // Start of a custom option
                in_custom_option = true;
                custom_option_brace_count = 0;
                // Count braces inside the custom option
                for ch in line.chars() {
                    if ch == '{' {
                        custom_option_brace_count += 1;
                    } else if ch == '}' {
                        custom_option_brace_count -= 1;
                    }
                }
                // Check if the custom option ends on the same line
                if trimmed.contains(']') && custom_option_brace_count <= 0 {
                    in_custom_option = false;
                }
            } else if in_custom_option {
                // Track braces inside custom option blocks
                for ch in line.chars() {
                    if ch == '{' {
                        custom_option_brace_count += 1;
                    } else if ch == '}' {
                        custom_option_brace_count -= 1;
                    }
                }

                // Check if we're exiting the custom option
                if trimmed.contains(']') && custom_option_brace_count <= 0 {
                    in_custom_option = false;
                }
            }

            // Check for common syntax errors (but not in custom options or block comments)
            if !trimmed.is_empty() && !trimmed.starts_with("//") && !in_custom_option {
                // Check for missing semicolons
                if trimmed.starts_with("package ") && !trimmed.ends_with(';') {
                    parse_errors.push(ParseError {
                        message: "Missing semicolon after package declaration".to_string(),
                        line: line_number,
                        character: line.len() as u32,
                        severity: ErrorSeverity::Error,
                    });
                }

                if trimmed.starts_with("import ") && !trimmed.ends_with(';') {
                    parse_errors.push(ParseError {
                        message: "Missing semicolon after import statement".to_string(),
                        line: line_number,
                        character: line.len() as u32,
                        severity: ErrorSeverity::Error,
                    });
                }

                // Check for invalid syntax
                if trimmed == "message" || trimmed == "enum" || trimmed == "service" {
                    parse_errors.push(ParseError {
                        message: format!("Missing name after {} declaration",
                            if trimmed == "message" { "message" }
                            else if trimmed == "enum" { "enum" }
                            else { "service" }),
                        line: line_number,
                        character: line.find(trimmed).unwrap_or(0) as u32,
                        severity: ErrorSeverity::Error,
                    });
                }
            }

            // Extract package
            if trimmed.starts_with("package ") {
                package = Some(
                    trimmed
                        .trim_start_matches("package ")
                        .trim_end_matches(';')
                        .trim()
                        .to_string(),
                );
            }

            // Extract imports
            else if trimmed.starts_with("import ") {
                let import_path = trimmed
                    .trim_start_matches("import ")
                    .trim_start_matches("\"")
                    .trim_end_matches("\";")
                    .trim_end_matches("\"")
                    .to_string();
                let import_char = processed_line.find("import").unwrap_or(0) as u32;
                imports.push(ImportElement {
                    path: import_path,
                    line: line_number,
                    character: import_char,
                });
            }

            // Extract enums
            else if trimmed.starts_with("enum ") {
                let enum_name = trimmed
                    .trim_start_matches("enum ")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();

                enum_stack.push((enum_name, line_number, Vec::new()));
            }
            // Extract messages
            else if trimmed.starts_with("message ") {
                let message_name = trimmed
                    .trim_start_matches("message ")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();

                message_stack.push((message_name, line_number, Vec::new(), Vec::new(), Vec::new()));
            }
            else if trimmed == "}" {
                // Handle enum closing
                if !enum_stack.is_empty() {
                    let (enum_name, start_line, values) = enum_stack.pop().unwrap();

                    let full_name = if let Some(pkg) = &package {
                        format!("{}.{}", pkg, enum_name)
                    } else {
                        enum_name.clone()
                    };

                    let original_line = content.lines().nth(start_line as usize).unwrap_or("");
                    let char_pos = original_line.find("enum").unwrap_or(0) as u32;

                    let enum_elem = EnumElement {
                        name: enum_name.clone(),
                        full_name,
                        values,
                        line: start_line,
                        end_line: line_number,
                        character: char_pos,
                    };

                    line_to_element.insert(start_line, ProtoElement::Enum(enum_elem.clone()));

                    if let Some(msg) = message_stack.last_mut() {
                        msg.4.push(enum_elem);
                    } else {
                        enums.push(enum_elem);
                    }
                }
                // Handle message closing
                else if !message_stack.is_empty() {
                    let (msg_name, start_line, fields, nested_msgs, nested_enums) = message_stack.pop().unwrap();

                    let full_name = if let Some(pkg) = &package {
                        format!("{}.{}", pkg, msg_name)
                    } else {
                        msg_name.clone()
                    };

                    let original_line = content.lines().nth(start_line as usize).unwrap_or("");
                    let char_pos = original_line.find("message").unwrap_or(0) as u32;

                    let msg = MessageElement {
                        name: msg_name.clone(),
                        full_name,
                        fields,
                        nested_messages: nested_msgs,
                        nested_enums,
                    line: start_line,
                    end_line: line_number,
                    character: char_pos,
                };

                    line_to_element.insert(start_line, ProtoElement::Message(msg.clone()));

                    if let Some(parent) = message_stack.last_mut() {
                        parent.3.push(msg);
                    } else {
                        messages.push(msg);
                    }
                }
            }
            // Extract services (check before field parsing since services can appear after messages)
            else if trimmed.starts_with("service") {
                // Clear any unclosed message stack first
                while !message_stack.is_empty() {
                    let (msg_name, start_line, fields, nested_msgs, nested_enums) = message_stack.pop().unwrap();
                    let full_name = if let Some(pkg) = &package {
                        format!("{}.{}", pkg, msg_name)
                    } else {
                        msg_name.clone()
                    };
                    let original_line = content.lines().nth(start_line as usize).unwrap_or("");
                    let char_pos = original_line.find("message").unwrap_or(0) as u32;
                    let msg = MessageElement {
                        name: msg_name.clone(),
                        full_name,
                        fields,
                        nested_messages: nested_msgs,
                        nested_enums,
                        line: start_line,
                        end_line: line_number - 1,
                        character: char_pos,
                    };
                    line_to_element.insert(start_line, ProtoElement::Message(msg.clone()));
                    messages.push(msg);
                }

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let service_name = parts[1].to_string();

                    let full_name = if let Some(pkg) = &package {
                        format!("{}.{}", pkg, service_name)
                    } else {
                        service_name.clone()
                    };

                    let char_pos = processed_line.find("service").unwrap_or(0) as u32;

                    // Parse the entire service block to extract methods
                    let service_content = Self::extract_service_block(content, line_number);
                    let methods = Self::parse_service_methods(&service_content, line_number);

                    let service_elem = ServiceElement {
                        name: service_name,
                        full_name,
                        methods,
                        line: line_number,
                        end_line: line_number,
                        character: char_pos,
                    };

                    line_to_element.insert(line_number, ProtoElement::Service(service_elem.clone()));
                    services.push(service_elem);
                }
            }
            else if !enum_stack.is_empty() && !trimmed.is_empty() && !trimmed.starts_with("//") {
                // Parse enum values
                if trimmed.contains('=') && trimmed.ends_with(';') {
                    let line_without_comment = if let Some(comment_pos) = trimmed.find("//") {
                        &trimmed[..comment_pos].trim()
                    } else {
                        trimmed
                    };

                    let parts: Vec<&str> = line_without_comment.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let value_name = parts[0].to_string();
                        if let Some(number_str) = parts.get(2) {
                            if let Ok(number) = number_str.trim_end_matches(';').parse::<i32>() {
                                if let Some(current_enum) = enum_stack.last_mut() {
                                    current_enum.2.push(EnumValueElement {
                                        name: value_name,
                                        number,
                                        line: line_number,
                                        character: 4,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            else if !message_stack.is_empty() && !trimmed.is_empty() && !trimmed.starts_with("//") && !in_custom_option {
                // Handle multiline field continuation
                if let Some((field_name, field_type, start_line)) = multiline_field.take() {
                    // This is a continuation of a multiline field
                    if trimmed.contains(';') {
                        // End of multiline field
                        let line_without_comment = if let Some(comment_pos) = trimmed.find("//") {
                            &trimmed[..comment_pos].trim()
                        } else {
                            trimmed
                        };

                        // Extract field number from this line
                        if let Some(number_str) = line_without_comment.trim_end_matches(';').split_whitespace().last() {
                            if let Ok(number) = number_str.parse::<i32>() {
                                if let Some(current_msg) = message_stack.last_mut() {
                                    current_msg.2.push(FieldElement {
                                        name: field_name,
                                        field_type,
                                        type_name: None,
                                        number,
                                        label: None,
                                        line: start_line,
                                        character: 0,
                                    });
                                }
                            } else {
                                parse_errors.push(ParseError {
                                    message: format!("Invalid field number: '{}'", number_str),
                                    line: line_number,
                                    character: 0,
                                    severity: ErrorSeverity::Error,
                                });
                            }
                        }
                    } else {
                        // Still not the end, keep the multiline field
                        multiline_field = Some((field_name, field_type, start_line));
                    }
                }
                // Check for the start of a multiline field
                else if trimmed.starts_with("optional ") || trimmed.starts_with("required ") || trimmed.starts_with("repeated ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let field_type = parts[1].to_string();
                        let field_name = parts[2].to_string();

                        // Check if this line ends with '=' (multiline field)
                        if trimmed.ends_with('=') {
                            multiline_field = Some((field_name, field_type, line_number));
                        }
                        // Try to parse regular field
                        else if trimmed.contains('=') && (trimmed.ends_with(';') || trimmed.contains(';')) {
                            if let Some(parts) = Self::parse_field_simple(trimmed, &processed_line) {
                                if let Some(current_msg) = message_stack.last_mut() {
                                    current_msg.2.push(FieldElement {
                                        name: parts.0,
                                        field_type: parts.1,
                                        type_name: None,
                                        number: parts.2,
                                        label: None,
                                        line: line_number,
                                        character: parts.3,
                                    });
                                }
                            } else {
                                parse_errors.push(ParseError {
                                    message: format!("Invalid field syntax: '{}'. Expected format: [optional|required|repeated] type name = number;", trimmed),
                                    line: line_number,
                                    character: 0,
                                    severity: ErrorSeverity::Error,
                                });
                            }
                        }
                    }
                }
                // Try to parse proto3 syntax fields (no label)
                else if trimmed.contains('=') && (trimmed.ends_with(';') || trimmed.contains(';')) {
                    // Check if this is a custom option (contains [ ... ])
                    if trimmed.contains('[') && trimmed.contains(']') {
                        // This might be a field with custom options, try to parse it
                        if let Some(parts) = Self::parse_field_simple(trimmed, &processed_line) {
                            if let Some(current_msg) = message_stack.last_mut() {
                                current_msg.2.push(FieldElement {
                                    name: parts.0,
                                    field_type: parts.1,
                                    type_name: None,
                                    number: parts.2,
                                    label: None,
                                    line: line_number,
                                    character: parts.3,
                                });
                            }
                        } else {
                            // Don't report error for custom options - they might be complex
                            // Just ignore it as it's likely valid protobuf syntax
                        }
                    } else if let Some(parts) = Self::parse_field_simple(trimmed, &processed_line) {
                        if let Some(current_msg) = message_stack.last_mut() {
                            current_msg.2.push(FieldElement {
                                name: parts.0,
                                field_type: parts.1,
                                type_name: None,
                                number: parts.2,
                                label: None,
                                line: line_number,
                                character: parts.3,
                            });
                        }
                    } else {
                        parse_errors.push(ParseError {
                            message: format!("Invalid field syntax: '{}'. Expected format: type name = number;", trimmed),
                            line: line_number,
                            character: 0,
                            severity: ErrorSeverity::Error,
                        });
                    }
                }
                // Check for proto3 optional keyword (which is invalid in proto3)
                else if trimmed.starts_with("optional ") && is_proto3 && !trimmed.contains('=') {
                    parse_errors.push(ParseError {
                        message: "'optional' keyword is not valid in proto3 syntax. In proto3, all fields are optional by default. Use 'optional' only for proto2 syntax or with 'oneof' in proto3.".to_string(),
                        line: line_number,
                        character: line.find("optional").unwrap_or(0) as u32,
                        severity: ErrorSeverity::Error,
                    });
                }
                // Check for other potential field errors
                else if !trimmed.starts_with("message ") && !trimmed.starts_with("enum ")
                    && !trimmed.starts_with("service ") && trimmed != "}"
                    && !trimmed.starts_with("//") && !trimmed.starts_with("/*")
                    && !trimmed.starts_with("option ") && !trimmed.starts_with("extend ")
                    && !trimmed.starts_with("rpc ") && !trimmed.starts_with("returns ")
                    && !trimmed.starts_with("map<") {
                    // Check if this looks like part of a custom option
                    if trimmed.contains(':') && (trimmed.contains("description:") || trimmed.contains("required:")
                        || trimmed.contains("hidden:") || trimmed.contains("default=")) {
                        // This is likely inside a custom option block, don't report error
                    } else if trimmed.starts_with("},") || trimmed.starts_with("}]") {
                        // This is closing a custom option block, don't report error
                    } else {
                        // Might be an invalid field line
                        if !trimmed.is_empty() && !trimmed.ends_with(';') && !trimmed.ends_with('{') && !trimmed.ends_with('}') {
                            parse_errors.push(ParseError {
                                message: format!("Unexpected syntax: '{}'. If this is a field, it should end with ';'", trimmed),
                                line: line_number,
                                character: 0,
                                severity: ErrorSeverity::Warning,
                            });
                        }
                    }
                }
            }

            current_line += 1;
        }

        Ok(ParsedProto {
            uri: uri.to_string(),
            package,
            imports,
            messages,
            enums,
            services,
            line_to_element,
            parse_errors,
            file_descriptor: None,
        })
    }

    /// Extract the entire service block content
    fn extract_service_block(content: &str, start_line: u32) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut block_lines = Vec::new();
        let mut brace_count = 0;
        let mut found_open = false;

        for (_i, line) in lines.iter().enumerate().skip(start_line as usize) {
            block_lines.push(*line);

            for ch in line.chars() {
                if ch == '{' {
                    brace_count += 1;
                    found_open = true;
                } else if ch == '}' {
                    brace_count -= 1;
                }
            }

            if found_open && brace_count == 0 {
                break;
            }
        }

        block_lines.join("\n")
    }

    /// Parse RPC methods from service block content
    fn parse_service_methods(service_content: &str, service_start_line: u32) -> Vec<MethodElement> {
        let mut methods = Vec::new();
        let mut in_block_comment = false;

        for (line_offset, line) in service_content.lines().enumerate() {
            let line_num = service_start_line + line_offset as u32;
            let trimmed = line.trim();

            // First check for line comments (//) - they take precedence over block comments
            if trimmed.starts_with("//") {
                continue; // Skip the entire line comment
            }

            // Handle block comment detection and stripping
            let processed_line = if in_block_comment {
                // We're inside a block comment, look for the end
                if let Some(end_pos) = line.find("*/") {
                    in_block_comment = false;
                    // Return the part after the block comment ends
                    line[end_pos + 2..].to_string()
                } else {
                    // Still inside block comment, return empty string to skip
                    String::new()
                }
            } else {
                // Not in a block comment, check if this line starts one
                if let Some(start_pos) = line.find("/*") {
                    if let Some(end_pos) = line[start_pos..].find("*/") {
                        // Block comment starts and ends on same line
                        // Remove the comment from the line
                        let comment_end = start_pos + end_pos + 2;
                        format!("{}{}",
                            &line[..start_pos],
                            &line[comment_end..])
                    } else {
                        // Block comment starts here and continues
                        in_block_comment = true;
                        // Return the part before the comment
                        line[..start_pos].to_string()
                    }
                } else {
                    // No block comment, return the line as-is
                    line.to_string()
                }
            };

            let trimmed = processed_line.trim();

            // Skip empty lines after comment processing
            if trimmed.is_empty() {
                continue;
            }

            // Look for rpc definitions
            if trimmed.starts_with("rpc ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 {
                    let method_name = parts.get(1).unwrap_or(&"").to_string();

                    // Extract input type (between parentheses)
                    if let Some(start) = trimmed.find('(') {
                        if let Some(end) = trimmed.find(')') {
                            let input_part = &trimmed[start + 1..end];
                            let input_type = input_part.split_whitespace().next().unwrap_or("").to_string();

                            // Extract output type (after "returns")
                            if let Some(returns_pos) = trimmed.find("returns") {
                                let returns_part = &trimmed[returns_pos + 7..];
                                if let Some(out_start) = returns_part.find('(') {
                                    if let Some(out_end) = returns_part.find(')') {
                                        let output_type = returns_part[out_start + 1..out_end]
                                            .split_whitespace()
                                            .next()
                                            .unwrap_or("")
                                            .to_string();

                                        let char_pos = processed_line.find("rpc").unwrap_or(0) as u32;

                                        methods.push(MethodElement {
                                            name: method_name,
                                            input_type,
                                            output_type,
                                            client_streaming: false, // TODO: Parse streaming modifiers
                                            server_streaming: false,
                                            line: line_num,
                                            character: char_pos,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        methods
    }

    fn parse_field_simple(line: &str, original_line: &str) -> Option<(String, String, i32, u32)> {
        // Handle both "name = value;" and "name=value;" formats
        let line_no_comment = if let Some(comment_pos) = line.find("//") {
            &line[..comment_pos].trim()
        } else {
            line
        };

        // Find the equals sign position
        let eq_pos = line_no_comment.find('=')?;
        let before_eq = &line_no_comment[..eq_pos].trim();
        let after_eq = &line_no_comment[eq_pos + 1..].trim();

        // Parse the parts before equals sign
        let parts_before: Vec<&str> = before_eq.split_whitespace().collect();

        let (field_type, field_name) = if parts_before.len() == 2 {
            // Format: "type name" (proto3 style)
            (parts_before[0], parts_before[1])
        } else if parts_before.len() == 3 {
            // Format: "optional type name" or "required type name" or "repeated type name"
            (parts_before[1], parts_before[2])
        } else {
            return None;
        };

        // Validate field type
        if !Self::is_valid_field_type(field_type) {
            return None;
        }

        // Extract field number from after equals (might be followed by options)
        let after_eq_parts: Vec<&str> = after_eq.splitn(2, '[').collect();
        let number_part = after_eq_parts[0].trim().trim_end_matches(';');
        let number = number_part.parse::<i32>().ok()?;

        let char_pos = original_line.find(field_name).unwrap_or(0) as u32;
        Some((field_name.to_string(), field_type.to_string(), number, char_pos))
    }

    /// Check if a string is a valid protobuf field type
    fn is_valid_field_type(s: &str) -> bool {
        // Basic types
        if matches!(s,
            "double" | "float" | "int32" | "int64" | "uint32" | "uint64" |
            "sint32" | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64" |
            "bool" | "string" | "bytes" | "map"
        ) {
            return true;
        }

        // Check if it's a message type (contains dots and starts with lowercase or uppercase)
        if s.contains('.') {
            return true;
        }

        // Check if it starts with uppercase (likely a message/enum type)
        if s.len() > 0 && s.chars().next().unwrap().is_uppercase() {
            return true;
        }

        false
    }

    /// Extract errors from protobuf-parse error
    fn extract_protobuf_parse_errors(&self, error: &anyhow::Error, total_lines: u32) -> Vec<ParseError> {
        let mut errors = Vec::new();
        let error_str = format!("{}", error);
        let mut line_numbers = std::collections::HashSet::new();

        // Pattern 0: ": at L:C:" (most common from protobuf-parse)
        if let Some(caps) = regex::Regex::new(r": at (\d+):(\d+):").unwrap().captures(&error_str) {
            if let (Ok(line), Ok(col)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                let message = self.extract_error_context(&error_str);
                errors.push(ParseError {
                    message,
                    line: line.saturating_sub(1),
                    character: col.saturating_sub(1),
                    severity: ErrorSeverity::Error,
                });
                line_numbers.insert(line);
                return errors;
            }
        }

        // Pattern 1: "in file.proto at line L:C"
        if let Some(caps) = regex::Regex::new(r"in .*? at line (\d+):(\d+)").unwrap().captures(&error_str) {
            if let (Ok(line), Ok(col)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                let message = self.clean_error_message(&error_str);
                errors.push(ParseError {
                    message,
                    line: line.saturating_sub(1),
                    character: col.saturating_sub(1),
                    severity: ErrorSeverity::Error,
                });
                line_numbers.insert(line);
                return errors;
            }
        }

        // Pattern 2: "error: line L:C"
        if let Some(caps) = regex::Regex::new(r"error: line (\d+):(\d+)").unwrap().captures(&error_str) {
            if let (Ok(line), Ok(col)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                let message = self.clean_error_message(&error_str);
                errors.push(ParseError {
                    message,
                    line: line.saturating_sub(1),
                    character: col.saturating_sub(1),
                    severity: ErrorSeverity::Error,
                });
                line_numbers.insert(line);
                return errors;
            }
        }

        // Pattern 3: "While parsing X, expecting Y at line L"
        if let Some(caps) = regex::Regex::new(r"While parsing .*? at line (\d+)").unwrap().captures(&error_str) {
            if let Ok(line) = caps[1].parse::<u32>() {
                let message = self.clean_error_message(&error_str);
                errors.push(ParseError {
                    message,
                    line: line.saturating_sub(1),
                    character: 0,
                    severity: ErrorSeverity::Error,
                });
                line_numbers.insert(line);
                return errors;
            }
        }

        // If no specific pattern matches, use the old recursive method
        let mut processed = std::collections::HashSet::new();
        self.extract_error_recursive(error, &mut errors, &mut processed, total_lines);

        errors
    }

    /// Recursively extract errors from error chain
    fn extract_error_recursive(
        &self,
        error: &anyhow::Error,
        errors: &mut Vec<ParseError>,
        processed: &mut std::collections::HashSet<String>,
        total_lines: u32,
    ) {
        let error_str = format!("{:?}", error);

        // Avoid duplicate errors
        if processed.contains(&error_str) {
            return;
        }
        processed.insert(error_str.clone());

        // Try to extract line and column information
        if let Some(line_col) = self.extract_line_column(&error_str) {
            let (line, column) = line_col;

            // Extract the actual error message
            let message = self.extract_error_message(&error_str);

            errors.push(ParseError {
                message,
                line: line.saturating_sub(1), // Convert to 0-based
                character: column.saturating_sub(1),
                severity: ErrorSeverity::Error,
            });
        } else {
            // If we can't extract line info, add a general error
            errors.push(ParseError {
                message: format!("Parse error: {}", error_str),
                line: 0,
                character: 0,
                severity: ErrorSeverity::Error,
            });
        }

        // Follow the error chain
        let mut source = error.source();
        while let Some(err) = source {
            // Convert to anyhow::Error if possible
            let anyhow_err = anyhow::anyhow!("{}", err);
            self.extract_error_recursive(&anyhow_err, errors, processed, total_lines);
            source = err.source();
        }
    }

    /// Extract line and column from error string
    fn extract_line_column(&self, error_str: &str) -> Option<(u32, u32)> {
        // Look for patterns like "at 7:5:" or "at line 7, column 5"
        use regex::Regex;

        // Pattern 1: "at 7:5:"
        if let Ok(re1) = Regex::new(r"at (\d+):(\d+):") {
            if let Some(caps) = re1.captures(error_str) {
                if let (Some(line), Some(col)) = (caps.get(1), caps.get(2)) {
                    if let (Ok(line_num), Ok(col_num)) = (line.as_str().parse::<u32>(), col.as_str().parse::<u32>()) {
                        return Some((line_num, col_num));
                    }
                }
            }
        }

        // Pattern 2: "line 7, column 5"
        if let Ok(re2) = Regex::new(r"line (\d+), column (\d+)") {
            if let Some(caps) = re2.captures(error_str) {
                if let (Some(line), Some(col)) = (caps.get(1), caps.get(2)) {
                    if let (Ok(line_num), Ok(col_num)) = (line.as_str().parse::<u32>(), col.as_str().parse::<u32>()) {
                        return Some((line_num, col_num));
                    }
                }
            }
        }

        None
    }

    /// Extract error context from protobuf-parse error
    fn extract_error_context(&self, error_str: &str) -> String {
        // Look for "expecting" pattern which gives the actual syntax error
        if let Some(pos) = error_str.find("expecting") {
            let context = &error_str[pos..];
            // Clean up the context
            let cleaned = context
                .split("at ")
                .next()
                .unwrap_or(context)
                .trim_end_matches(':')
                .trim();

            if cleaned.starts_with("expecting") {
                format!("Syntax error: {}", cleaned)
            } else {
                cleaned.to_string()
            }
        } else if error_str.contains("unexpected token") {
            "Unexpected token".to_string()
        } else if error_str.contains("unexpected") {
            "Unexpected syntax".to_string()
        } else {
            "Parse error".to_string()
        }
    }

    /// Clean error message to extract the meaningful part
    fn clean_error_message(&self, error_str: &str) -> String {
        // Remove file path and position info, keep only the actual message
        let msg = error_str
            .split("While parsing")
            .next()
            .unwrap_or(error_str)
            .split("Caused by:")
            .next()
            .unwrap_or(error_str)
            .trim();

        // Remove common prefixes and patterns
        let cleaned = msg
            .split("error in")
            .last()
            .unwrap_or(msg)
            .split("protobuf path")
            .last()
            .unwrap_or(msg)
            .split("is not found in import path")
            .next()
            .unwrap_or(msg)
            .trim();

        // If it starts with "expected", add context
        if cleaned.starts_with("expected") {
            format!("Syntax error: {}", cleaned)
        } else {
            cleaned.to_string()
        }
    }

    /// Extract clean error message
    fn extract_error_message(&self, error_str: &str) -> String {
        self.clean_error_message(error_str)
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl Default for ProtoParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy implementation for backward compatibility
impl ParsedProto {
    /// Parse a protobuf file using the new parser
    pub async fn parse(uri: String, content: &str) -> Result<Self> {
        let parser = ProtoParser::new();
        parser.parse(uri, content).await
    }

    /// Find element at position
    pub fn find_element_at_position(&self, position: Position) -> Option<&ProtoElement> {
        self.line_to_element.get(&position.line)
    }

    /// Find message by name
    pub fn find_message_by_name(&self, name: &str) -> Option<&MessageElement> {
        self.find_message_recursive(&self.messages, name)
    }

    fn find_message_recursive<'a>(
        &'a self,
        messages: &'a [MessageElement],
        name: &str,
    ) -> Option<&'a MessageElement> {
        for msg in messages {
            if msg.name == name || msg.full_name == name {
                return Some(msg);
            }
            if let Some(found) = self.find_message_recursive(&msg.nested_messages, name) {
                return Some(found);
            }
        }
        None
    }

    /// Find enum by name
    pub fn find_enum_by_name(&self, name: &str) -> Option<&EnumElement> {
        self.find_enum_recursive(&self.enums, name)
    }

    fn find_enum_recursive<'a>(
        &'a self,
        enums: &'a [EnumElement],
        name: &str,
    ) -> Option<&'a EnumElement> {
        for e in enums {
            if e.name == name || e.full_name == name {
                return Some(e);
            }
        }
        for msg in &self.messages {
            if let Some(found) = self.find_enum_in_message(msg, name) {
                return Some(found);
            }
        }
        None
    }

    fn find_enum_in_message<'a>(
        &'a self,
        msg: &'a MessageElement,
        name: &str,
    ) -> Option<&'a EnumElement> {
        for e in &msg.nested_enums {
            if e.name == name || e.full_name == name {
                return Some(e);
            }
        }
        for nested_msg in &msg.nested_messages {
            if let Some(found) = self.find_enum_in_message(nested_msg, name) {
                return Some(found);
            }
        }
        None
    }

    /// Find service by name
    pub fn find_service_by_name(&self, name: &str) -> Option<&ServiceElement> {
        self.services
            .iter()
            .find(|s| s.name == name || s.full_name == name)
    }

    /// Find method by name in any service
    pub fn find_method_by_name(&self, name: &str) -> Option<(&ServiceElement, &MethodElement)> {
        for service in &self.services {
            if let Some(method) = service.methods.iter().find(|m| m.name == name) {
                return Some((service, method));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_with_service() {
        let content = r#"
syntax = "proto3";
package test;

service UserService {
    rpc GetUser(GetUserRequest) returns (GetUserResponse);
    rpc ListUsers(ListUsersRequest) returns (stream ListUsersResponse);
    rpc UpdateUser(stream UpdateUserRequest) returns (UpdateUserResponse);
}

message GetUserRequest {
    string user_id = 1;
}

message GetUserResponse {
    User user = 1;
}

message User {
    string id = 1;
    string name = 2;
    int32 age = 3;
}
"#;

        let result = ParsedProto::parse("test.proto".to_string(), content).await;
        assert!(result.is_ok());

        let proto = result.unwrap();
        assert_eq!(proto.package, Some("test".to_string()));
        assert_eq!(proto.services.len(), 1);

        let service = &proto.services[0];
        assert_eq!(service.name, "UserService");
        assert_eq!(service.methods.len(), 3);

        let method = &service.methods[0];
        assert_eq!(method.name, "GetUser");
        assert_eq!(method.input_type, ".test.GetUserRequest");
        assert_eq!(method.output_type, ".test.GetUserResponse");
        assert!(!method.client_streaming);
        assert!(!method.server_streaming);

        let method = &service.methods[1];
        assert_eq!(method.name, "ListUsers");
        assert!(!method.client_streaming);
        assert!(method.server_streaming);

        let method = &service.methods[2];
        assert_eq!(method.name, "UpdateUser");
        assert!(method.client_streaming);
        assert!(!method.server_streaming);
    }

    #[tokio::test]
    async fn test_parse_nested_messages() {
        let content = r#"
syntax = "proto3";
package test;

message Outer {
    string outer_field = 1;

    message Inner {
        string inner_field = 1;

        message Deepest {
            int32 deep_field = 1;
        }
    }
}
"#;

        let result = ParsedProto::parse("test.proto".to_string(), content).await;
        assert!(result.is_ok());

        let proto = result.unwrap();
        assert_eq!(proto.messages.len(), 1);

        let outer = &proto.messages[0];
        assert_eq!(outer.name, "Outer");
        assert_eq!(outer.nested_messages.len(), 1);

        let inner = &outer.nested_messages[0];
        assert_eq!(inner.name, "Inner");
        assert_eq!(inner.nested_messages.len(), 1);

        let deepest = &inner.nested_messages[0];
        assert_eq!(deepest.name, "Deepest");
    }
}