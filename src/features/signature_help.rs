use crate::workspace::WorkspaceManager;
use tower_lsp::lsp_types::*;

/// Provide signature help for RPC method definitions.
/// Triggered when the cursor is after `(` in `rpc MethodName(` or `returns (`.
pub fn provide_signature_help(
    params: SignatureHelpParams,
    workspace: &WorkspaceManager,
    content: Option<&str>,
) -> Option<SignatureHelp> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let proto = workspace.get_file(uri)?;
    let content = content?;

    let line_str = content.lines().nth(position.line as usize)?;
    let before_cursor = &line_str[..position.character as usize];

    // Check if we're inside an rpc definition
    let trimmed = line_str.trim();
    if !trimmed.starts_with("rpc ") {
        return None;
    }

    // Find which method we're in
    let method_name = extract_rpc_method_name(trimmed)?;

    // Look up the method in parsed services
    let (service, method) = proto.find_method_by_name(&method_name)?;

    // Determine if cursor is in input type or return type position
    let active_parameter = if before_cursor.contains("returns") {
        Some(1u32)
    } else {
        Some(0u32)
    };

    let input_label = format_type_label(&method.input_type, method.client_streaming);
    let output_label = format_type_label(&method.output_type, method.server_streaming);

    let signature_label = format!(
        "rpc {}({}) returns ({})",
        method.name, input_label, output_label
    );

    let parameters = vec![
        ParameterInformation {
            label: ParameterLabel::Simple(input_label.clone()),
            documentation: Some(Documentation::String(format!(
                "Input type: {}",
                method.input_type
            ))),
        },
        ParameterInformation {
            label: ParameterLabel::Simple(output_label.clone()),
            documentation: Some(Documentation::String(format!(
                "Output type: {}",
                method.output_type
            ))),
        },
    ];

    let signature = SignatureInformation {
        label: signature_label,
        documentation: Some(Documentation::String(format!(
            "RPC method in service {}",
            service.name
        ))),
        parameters: Some(parameters),
        active_parameter,
    };

    Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter,
    })
}

/// Extract the method name from an rpc line like `rpc GetUser(GetUserRequest) returns (...);`
fn extract_rpc_method_name(line: &str) -> Option<String> {
    let after_rpc = line.strip_prefix("rpc ")?.trim_start();
    let end = after_rpc.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')?;
    let name = &after_rpc[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Format a type label, adding `stream` prefix if applicable.
fn format_type_label(type_name: &str, streaming: bool) -> String {
    let short_name = type_name
        .rsplit('.')
        .next()
        .unwrap_or(type_name);
    if streaming {
        format!("stream {}", short_name)
    } else {
        short_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rpc_method_name() {
        assert_eq!(
            extract_rpc_method_name("rpc GetUser(GetUserRequest) returns (GetUserResponse);"),
            Some("GetUser".to_string())
        );
        assert_eq!(
            extract_rpc_method_name("rpc ListUsers(ListUsersRequest) returns (stream ListUsersResponse);"),
            Some("ListUsers".to_string())
        );
        assert_eq!(extract_rpc_method_name("message Foo {"), None);
    }

    #[test]
    fn test_format_type_label() {
        assert_eq!(format_type_label(".test.GetUserRequest", false), "GetUserRequest");
        assert_eq!(format_type_label(".test.ListUsersResponse", true), "stream ListUsersResponse");
    }
}
