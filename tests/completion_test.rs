use protobuf_lsp::features::provide_completion;
use protobuf_lsp::workspace::WorkspaceManager;
use tower_lsp::lsp_types::{
    CompletionParams, CompletionResponse, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};

async fn run_completion(
    workspace: &WorkspaceManager,
    uri: &Url,
    content: &str,
    position: Position,
) -> Vec<String> {
    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        context: None,
        work_done_progress_params: WorkDoneProgressParams {
            work_done_token: None,
        },
        partial_result_params: PartialResultParams {
            partial_result_token: None,
        },
    };

    let response = provide_completion(params, workspace, Some(content))
        .await
        .expect("completion response");

    match response {
        CompletionResponse::Array(items) => items.into_iter().map(|i| i.label).collect(),
        _ => panic!("expected CompletionResponse::Array"),
    }
}

/// Build a workspace with a file that first parses successfully, then is updated
/// to an in-progress (un-parseable) state. Returns the workspace and the URI.
async fn workspace_with_in_progress_edit() -> (WorkspaceManager, Url) {
    let workspace = WorkspaceManager::new();
    let url = Url::parse("file:///test/test.proto").unwrap();

    // First, open a complete, valid proto file. This seeds the "last good" cache.
    let complete = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
}

message Bar {
    string value = 1;
}

enum Color {
    RED = 0;
    GREEN = 1;
}
"#;
    workspace.open_file(&url, complete).await.unwrap();

    // Now the user starts typing a new field inside Foo, leaving the file
    // syntactically incomplete. proto-parser will fail to parse this; the
    // workspace should keep serving the last good result.
    let in_progress = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
    Bar 
}

message Bar {
    string value = 1;
}

enum Color {
    RED = 0;
    GREEN = 1;
}
"#;
    // This call should NOT panic / error: the workspace keeps the last good result.
    workspace.open_file(&url, in_progress).await.unwrap();

    (workspace, url)
}

#[tokio::test]
async fn test_completion_includes_existing_messages_after_incomplete_edit() {
    let (workspace, url) = workspace_with_in_progress_edit().await;

    // The in-progress content: cursor right after "Bar " on line 5.
    let in_progress = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
    Bar 
}

message Bar {
    string value = 1;
}

enum Color {
    RED = 0;
    GREEN = 1;
}
"#;
    let labels = run_completion(
        &workspace,
        &url,
        in_progress,
        Position { line: 5, character: 7 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    // Bar must be present (it was parsed in the last good result).
    assert!(
        labels.iter().any(|l| l == "Bar"),
        "expected 'Bar' (existing message) to be in completion list, got: {:?}",
        labels
    );
    // Foo must also be present.
    assert!(
        labels.iter().any(|l| l == "Foo"),
        "expected 'Foo' (existing message) to be in completion list, got: {:?}",
        labels
    );
}

#[tokio::test]
async fn test_completion_includes_enums_after_incomplete_edit() {
    let (workspace, url) = workspace_with_in_progress_edit().await;

    let in_progress = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
    Bar 
}

message Bar {
    string value = 1;
}

enum Color {
    RED = 0;
    GREEN = 1;
}
"#;
    let labels = run_completion(
        &workspace,
        &url,
        in_progress,
        Position { line: 5, character: 7 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    assert!(
        labels.iter().any(|l| l == "Color"),
        "expected 'Color' (existing enum) to be in completion list, got: {:?}",
        labels
    );
}

#[tokio::test]
async fn test_completion_returns_nonempty_after_incomplete_edit() {
    let (workspace, url) = workspace_with_in_progress_edit().await;

    let in_progress = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
    Bar 
}

message Bar {
    string value = 1;
}
"#;
    // Cursor on the empty line inside Foo (line 4 — the blank line before the field).
    let labels = run_completion(
        &workspace,
        &url,
        in_progress,
        Position { line: 4, character: 4 },
    )
    .await;
    println!("completion labels: {:?}", labels);
    assert!(!labels.is_empty(), "completion list should not be empty");
}

#[tokio::test]
async fn test_first_open_invalid_file_has_no_completion() {
    // If the very first parse of a file fails (no last good result), completion
    // should gracefully return None rather than crashing.
    let workspace = WorkspaceManager::new();
    let url = Url::parse("file:///test/bad.proto").unwrap();

    let broken = "this is not valid proto at all";
    // First open fails; open_file returns Err because there is no last good.
    let result = workspace.open_file(&url, broken).await;
    assert!(result.is_err());

    // No completion available.
    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: url.clone() },
            position: Position { line: 0, character: 0 },
        },
        context: None,
        work_done_progress_params: WorkDoneProgressParams {
            work_done_token: None,
        },
        partial_result_params: PartialResultParams {
            partial_result_token: None,
        },
    };
    let response = provide_completion(params, &workspace, Some(broken)).await;
    assert!(response.is_none(), "expected no completion for never-parsed file");
}

#[tokio::test]
async fn test_get_last_errors_records_parse_failure() {
    let (workspace, url) = workspace_with_in_progress_edit().await;
    // After the in-progress edit, last_errors should contain one parse error.
    let errors = workspace.get_last_errors(&url);
    assert_eq!(errors.len(), 1, "expected exactly one parse error, got: {:?}", errors);
    assert!(!errors[0].message.is_empty());
}

#[tokio::test]
async fn test_get_last_errors_cleared_on_successful_reparse() {
    let (workspace, url) = workspace_with_in_progress_edit().await;
    assert_eq!(workspace.get_last_errors(&url).len(), 1);

    // Re-open with a valid file; errors should clear.
    let complete = r#"syntax = "proto3";
package test;

message Foo {
    string name = 1;
}
"#;
    workspace.open_file(&url, complete).await.unwrap();
    assert!(workspace.get_last_errors(&url).is_empty());
}
