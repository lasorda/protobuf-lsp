use anyhow::Result;
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
#[allow(dead_code)]
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
    pub extends: Vec<ExtendElement>,
    pub line_to_element: HashMap<u32, ProtoElement>,
    /// Parse errors collected during parsing
    pub parse_errors: Vec<ParseError>,
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
#[allow(dead_code)]
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

/// Extend definition element - represents `extend SomeMessage { ... }`
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExtendElement {
    pub name: String,       // The message name being extended (e.g. "Base")
    pub full_name: String,  // Fully-qualified name
    pub fields: Vec<FieldElement>,
    pub line: u32,
    pub end_line: u32,
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
#[allow(dead_code)]
pub enum ProtoElement {
    Message(MessageElement),
    Enum(EnumElement),
    Service(ServiceElement),
    Field(FieldElement),
    Method(MethodElement),
}

/// Parser for protobuf files using proto-parser library
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

        let parse_result = match proto_parser::Parser::new(content).parse() {
            Ok(proto) => self.convert_proto(&uri, &proto),
            Err(e) => {
                // Parse failed — return empty result with error
                let line = if e.position.line > 0 {
                    e.position.line as u32 - 1
                } else {
                    0
                };
                let character = if e.position.column > 0 {
                    e.position.column as u32 - 1
                } else {
                    0
                };
                ParsedProto {
                    uri: uri.clone(),
                    package: None,
                    imports: Vec::new(),
                    messages: Vec::new(),
                    enums: Vec::new(),
                    services: Vec::new(),
                    extends: Vec::new(),
                    line_to_element: HashMap::new(),
                    parse_errors: vec![ParseError {
                        message: e.message.clone(),
                        line,
                        character,
                        severity: ErrorSeverity::Error,
                    }],
                }
            }
        };

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(uri.clone(), parse_result.clone());
        }

        Ok(parse_result)
    }

    /// Convert proto-rs AST to our ParsedProto representation
    fn convert_proto(&self, uri: &str, proto: &proto_parser::Proto) -> ParsedProto {
        let mut package: Option<String> = None;
        let mut imports = Vec::new();
        let mut messages = Vec::new();
        let mut enums = Vec::new();
        let mut services = Vec::new();
        let mut extends = Vec::new();
        let mut line_to_element = HashMap::new();

        for element in &proto.elements {
            match element {
                proto_parser::Element::Package(p) => {
                    package = Some(p.name.clone());
                }
                proto_parser::Element::Import(i) => {
                    imports.push(ImportElement {
                        path: i.filename.clone(),
                        line: pos_line(i.position.line),
                        character: pos_col(i.position.column),
                    });
                }
                proto_parser::Element::Message(m) => {
                    if m.is_extend {
                        let ext = self.convert_extend(m, &package);
                        extends.push(ext);
                    } else {
                        let msg = self.convert_message(m, &package, "");
                        line_to_element.insert(msg.line, ProtoElement::Message(msg.clone()));
                        messages.push(msg);
                    }
                }
                proto_parser::Element::Enum(e) => {
                    let enum_elem = self.convert_enum(e, &package, "");
                    line_to_element.insert(enum_elem.line, ProtoElement::Enum(enum_elem.clone()));
                    enums.push(enum_elem);
                }
                proto_parser::Element::Service(s) => {
                    let service = self.convert_service(s, &package);
                    line_to_element
                        .insert(service.line, ProtoElement::Service(service.clone()));
                    services.push(service);
                }
                _ => {} // Syntax, Option, Comment, etc. — not needed by LSP features
            }
        }

        ParsedProto {
            uri: uri.to_string(),
            package,
            imports,
            messages,
            enums,
            services,
            extends,
            line_to_element,
            parse_errors: Vec::new(),
        }
    }

    /// Convert a proto-rs Message with is_extend=true to ExtendElement
    fn convert_extend(
        &self,
        m: &proto_parser::Message,
        package: &Option<String>,
    ) -> ExtendElement {
        let name = m.name.clone();
        let full_name = if let Some(pkg) = package {
            format!("{}.{}", pkg, name)
        } else {
            name.clone()
        };

        let mut fields = Vec::new();
        let mut last_line = pos_line(m.position.line);

        for elem in &m.elements {
            match elem {
                proto_parser::Element::NormalField(f) => {
                    let fe = self.convert_normal_field(f);
                    if fe.line > last_line {
                        last_line = fe.line;
                    }
                    fields.push(fe);
                }
                _ => {}
            }
        }

        let end_line = if last_line > pos_line(m.position.line) {
            last_line + 1
        } else {
            pos_line(m.position.line) + 1
        };

        ExtendElement {
            name,
            full_name,
            fields,
            line: pos_line(m.position.line),
            end_line,
            character: pos_col(m.position.column),
        }
    }

    /// Convert a proto-rs Message to MessageElement
    fn convert_message(
        &self,
        m: &proto_parser::Message,
        package: &Option<String>,
        parent_name: &str,
    ) -> MessageElement {
        let name = m.name.clone();
        let full_name = make_full_name(package, parent_name, &name);

        let mut fields = Vec::new();
        let mut nested_messages = Vec::new();
        let mut nested_enums = Vec::new();
        let mut last_line = pos_line(m.position.line);

        for elem in &m.elements {
            match elem {
                proto_parser::Element::NormalField(f) => {
                    let fe = self.convert_normal_field(f);
                    if fe.line > last_line {
                        last_line = fe.line;
                    }
                    fields.push(fe);
                }
                proto_parser::Element::MapField(f) => {
                    let fe = self.convert_map_field(f);
                    if fe.line > last_line {
                        last_line = fe.line;
                    }
                    fields.push(fe);
                }
                proto_parser::Element::Oneof(o) => {
                    // Flatten oneof fields into the message fields list
                    for oe in &o.elements {
                        if let proto_parser::Element::OneofField(of) = oe {
                            let fe = self.convert_oneof_field(of);
                            if fe.line > last_line {
                                last_line = fe.line;
                            }
                            fields.push(fe);
                        }
                    }
                }
                proto_parser::Element::Message(nested_m) => {
                    // Skip nested extend blocks — they are references, not definitions
                    if !nested_m.is_extend {
                        let nested = self.convert_message(nested_m, package, &full_name);
                        if nested.end_line > last_line {
                            last_line = nested.end_line;
                        }
                        nested_messages.push(nested);
                    }
                }
                proto_parser::Element::Enum(nested_e) => {
                    let nested = self.convert_enum(nested_e, package, &full_name);
                    if nested.end_line > last_line {
                        last_line = nested.end_line;
                    }
                    nested_enums.push(nested);
                }
                _ => {}
            }
        }

        // Estimate end_line: use the last child element's line + 1 for the closing brace
        let end_line = if last_line > pos_line(m.position.line) {
            last_line + 1
        } else {
            pos_line(m.position.line) + 1
        };

        MessageElement {
            name,
            full_name,
            fields,
            nested_messages,
            nested_enums,
            line: pos_line(m.position.line),
            end_line,
            character: pos_col(m.position.column),
        }
    }

    /// Convert a proto-rs NormalField to FieldElement
    fn convert_normal_field(&self, f: &proto_parser::NormalField) -> FieldElement {
        let label = if f.repeated {
            Some(FieldLabelProto::Repeated)
        } else if f.optional {
            Some(FieldLabelProto::Optional)
        } else if f.required {
            Some(FieldLabelProto::Required)
        } else {
            None
        };

        let type_name = if is_builtin_type(&f.field.type_name) {
            None
        } else {
            Some(f.field.type_name.clone())
        };

        FieldElement {
            name: f.field.name.clone(),
            field_type: f.field.type_name.clone(),
            type_name,
            number: f.field.sequence as i32,
            label,
            line: pos_line(f.field.position.line),
            character: pos_col(f.field.position.column),
        }
    }

    /// Convert a proto-rs MapField to FieldElement
    fn convert_map_field(&self, f: &proto_parser::MapField) -> FieldElement {
        let map_type = format!("map<{}, {}>", f.key_type, f.field.type_name);
        FieldElement {
            name: f.field.name.clone(),
            field_type: map_type,
            type_name: None,
            number: f.field.sequence as i32,
            label: Some(FieldLabelProto::Repeated),
            line: pos_line(f.field.position.line),
            character: pos_col(f.field.position.column),
        }
    }

    /// Convert a proto-rs OneofField to FieldElement
    fn convert_oneof_field(&self, f: &proto_parser::OneofField) -> FieldElement {
        let type_name = if is_builtin_type(&f.field.type_name) {
            None
        } else {
            Some(f.field.type_name.clone())
        };

        FieldElement {
            name: f.field.name.clone(),
            field_type: f.field.type_name.clone(),
            type_name,
            number: f.field.sequence as i32,
            label: None,
            line: pos_line(f.field.position.line),
            character: pos_col(f.field.position.column),
        }
    }

    /// Convert a proto-rs Enum to EnumElement
    fn convert_enum(
        &self,
        e: &proto_parser::Enum,
        package: &Option<String>,
        parent_name: &str,
    ) -> EnumElement {
        let name = e.name.clone();
        let full_name = make_full_name(package, parent_name, &name);

        let mut values = Vec::new();
        let mut last_line = pos_line(e.position.line);

        for elem in &e.elements {
            if let proto_parser::Element::EnumField(ef) = elem {
                let line = pos_line(ef.position.line);
                if line > last_line {
                    last_line = line;
                }
                values.push(EnumValueElement {
                    name: ef.name.clone(),
                    number: ef.integer as i32,
                    line,
                    character: pos_col(ef.position.column),
                });
            }
        }

        let end_line = if last_line > pos_line(e.position.line) {
            last_line + 1
        } else {
            pos_line(e.position.line) + 1
        };

        EnumElement {
            name,
            full_name,
            values,
            line: pos_line(e.position.line),
            end_line,
            character: pos_col(e.position.column),
        }
    }

    /// Convert a proto-rs Service to ServiceElement
    fn convert_service(
        &self,
        s: &proto_parser::Service,
        package: &Option<String>,
    ) -> ServiceElement {
        let name = s.name.clone();
        let full_name = if let Some(pkg) = package {
            format!("{}.{}", pkg, name)
        } else {
            name.clone()
        };

        let mut methods = Vec::new();
        let mut last_line = pos_line(s.position.line);

        for elem in &s.elements {
            if let proto_parser::Element::Rpc(rpc) = elem {
                let line = pos_line(rpc.position.line);
                if line > last_line {
                    last_line = line;
                }

                // Qualify the type names with package prefix (matching protobuf convention)
                let input_type = qualify_type_name(&rpc.request_type, package);
                let output_type = qualify_type_name(&rpc.returns_type, package);

                methods.push(MethodElement {
                    name: rpc.name.clone(),
                    input_type,
                    output_type,
                    client_streaming: rpc.streams_request,
                    server_streaming: rpc.streams_returns,
                    line,
                    character: pos_col(rpc.position.column),
                });
            }
        }

        let end_line = if last_line > pos_line(s.position.line) {
            last_line + 1
        } else {
            pos_line(s.position.line) + 1
        };

        ServiceElement {
            name,
            full_name,
            methods,
            line: pos_line(s.position.line),
            end_line,
            character: pos_col(s.position.column),
        }
    }

    /// Clear the cache
    #[allow(dead_code)]
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

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Convert proto-rs 1-based line to LSP 0-based line
fn pos_line(line: usize) -> u32 {
    if line > 0 { line as u32 - 1 } else { 0 }
}

/// Convert proto-rs 1-based column to LSP 0-based column
fn pos_col(col: usize) -> u32 {
    if col > 0 { col as u32 - 1 } else { 0 }
}

/// Build a fully-qualified name like "package.Parent.Name"
fn make_full_name(package: &Option<String>, parent_name: &str, name: &str) -> String {
    if let Some(pkg) = package {
        if parent_name.is_empty() {
            format!("{}.{}", pkg, name)
        } else {
            format!("{}.{}.{}", pkg, parent_name, name)
        }
    } else if parent_name.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", parent_name, name)
    }
}

/// Qualify a type name with a leading dot and package prefix.
/// E.g., "GetUserRequest" with package "test" → ".test.GetUserRequest"
/// Already-qualified names (starting with ".") are left as-is.
fn qualify_type_name(type_name: &str, package: &Option<String>) -> String {
    if type_name.starts_with('.') {
        // Already fully qualified
        return type_name.to_string();
    }
    if let Some(pkg) = package {
        format!(".{}.{}", pkg, type_name)
    } else {
        format!(".{}", type_name)
    }
}

/// Check whether a type string is a protobuf built-in scalar type
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

// ---------------------------------------------------------------------------
// Legacy / utility methods on ParsedProto
// ---------------------------------------------------------------------------

impl ParsedProto {
    /// Parse a protobuf file using the new parser
    #[allow(dead_code)]
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

    /// Find a field by name inside any extend block
    pub fn find_extend_field_by_name(&self, name: &str) -> Option<(&ExtendElement, &FieldElement)> {
        for ext in &self.extends {
            if let Some(field) = ext.fields.iter().find(|f| f.name == name) {
                return Some((ext, field));
            }
        }
        None
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

    #[tokio::test]
    async fn test_parse_extend() {
        let content = r#"
syntax = "proto2";
package test;

message Base {
    optional string name = 1;
}

extend Base {
    optional int32 extra_field = 100;
}

message Other {
    optional string value = 1;
}
"#;

        let result = ParsedProto::parse("test.proto".to_string(), content).await;
        assert!(result.is_ok());

        let proto = result.unwrap();

        // extend should NOT appear in messages list
        assert_eq!(proto.messages.len(), 2, "Should have exactly 2 messages (Base and Other), not the extend");
        assert_eq!(proto.messages[0].name, "Base");
        assert_eq!(proto.messages[1].name, "Other");

        // extend should appear in extends list
        assert_eq!(proto.extends.len(), 1);
        assert_eq!(proto.extends[0].name, "Base");
        assert_eq!(proto.extends[0].full_name, "test.Base");
        assert_eq!(proto.extends[0].fields.len(), 1);
        assert_eq!(proto.extends[0].fields[0].name, "extra_field");

        // find_message_by_name should find the real Base message, not the extend
        let base_msg = proto.find_message_by_name("Base");
        assert!(base_msg.is_some());
        let base_msg = base_msg.unwrap();
        assert_eq!(base_msg.fields.len(), 1);
        assert_eq!(base_msg.fields[0].name, "name");
    }

    #[tokio::test]
    async fn test_extend_field_lookup() {
        // Simulates the real-world scenario:
        // skbuiltintype.proto defines: extend google.protobuf.MethodOptions { optional string RpcRouteMethod = ...; }
        // mmsearchmcpproxy.proto uses: option (tlvpickle.RpcRouteMethod) = "kConHash";
        let content = r#"
syntax = "proto2";
package tlvpickle;

message MethodOptions {
    optional string name = 1;
}

extend MethodOptions {
    optional int32 CmdID = 1000000;
    optional string RpcRouteMethod = 1000015;
    optional string Brief = 1000005;
}
"#;

        let result = ParsedProto::parse("skbuiltintype.proto".to_string(), content).await;
        assert!(result.is_ok());

        let proto = result.unwrap();

        // extend should NOT be in messages
        assert_eq!(proto.messages.len(), 1);
        assert_eq!(proto.messages[0].name, "MethodOptions");

        // extend should be in extends
        assert_eq!(proto.extends.len(), 1);
        assert_eq!(proto.extends[0].name, "MethodOptions");
        assert_eq!(proto.extends[0].fields.len(), 3);

        // find_extend_field_by_name should find RpcRouteMethod
        let result = proto.find_extend_field_by_name("RpcRouteMethod");
        assert!(result.is_some());
        let (ext, field) = result.unwrap();
        assert_eq!(ext.name, "MethodOptions");
        assert_eq!(field.name, "RpcRouteMethod");

        // find_extend_field_by_name should find CmdID
        let result = proto.find_extend_field_by_name("CmdID");
        assert!(result.is_some());

        // Should NOT find non-existent field
        let result = proto.find_extend_field_by_name("NonExistent");
        assert!(result.is_none());
    }
}
