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

/// Repro for: completion does not suggest imported proto files' package names.
///
/// Setup: a main proto file imports another proto file that declares a different
/// package. When the user types the imported package's prefix (e.g. `other.`),
/// the LSP should suggest `other.` as a package completion item.
#[tokio::test]
async fn test_completion_suggests_imported_package_name() {
    use std::fs;
    use std::path::PathBuf;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let dir = tempfile::tempdir().expect("create tempdir");
    let dir_path = dir.path().to_path_buf();

    // Write the imported proto file with package `other`.
    let imported_path: PathBuf = dir_path.join("other.proto");
    let imported_content = r#"syntax = "proto3";
package other;

message OtherMessage {
    string name = 1;
}
"#;
    fs::write(&imported_path, imported_content).unwrap();

    // Write the main proto file that imports `other.proto`.
    let main_path: PathBuf = dir_path.join("main.proto");
    let main_content = r#"syntax = "proto3";
package main;

import "other.proto";

message MainMessage {
    string name = 1;
}
"#;
    fs::write(&main_path, main_content).unwrap();

    let workspace = WorkspaceManager::new();
    let main_uri = Url::from_file_path(&main_path).unwrap();
    workspace.open_file(&main_uri, main_content).await.unwrap();

    // The user starts typing the imported package name `o` at top level.
    // We re-open with the in-progress content where the cursor is right after `o`.
    let in_progress = r#"syntax = "proto3";
package main;

import "other.proto";

o
"#;
    workspace.open_file(&main_uri, in_progress).await.unwrap();

    // Cursor right after `o` at top level (line 5, character 1).
    let labels = run_completion(
        &workspace,
        &main_uri,
        in_progress,
        Position { line: 5, character: 1 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    // The imported package `other.` should be suggested.
    assert!(
        labels.iter().any(|l| l == "other."),
        "expected 'other.' (imported package name) to be in completion list, got: {:?}",
        labels
    );
}

/// Repro for: when typing a field type inside a message, the LSP should still
/// suggest imported package names so the user can write `other.OtherMessage`.
///
/// This is the most common real-world scenario: the user is inside a message
/// body, typing the type of a field, and wants to reference a symbol from an
/// imported proto file by its fully-qualified package path.
#[tokio::test]
async fn test_completion_suggests_imported_package_inside_message() {
    use std::fs;
    use std::path::PathBuf;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let dir = tempfile::tempdir().expect("create tempdir");
    let dir_path = dir.path().to_path_buf();

    let imported_path: PathBuf = dir_path.join("other.proto");
    let imported_content = r#"syntax = "proto3";
package other;

message OtherMessage {
    string name = 1;
}
"#;
    fs::write(&imported_path, imported_content).unwrap();

    let main_path: PathBuf = dir_path.join("main.proto");
    let main_content = r#"syntax = "proto3";
package main;

import "other.proto";

message MainMessage {
    string name = 1;
}
"#;
    fs::write(&main_path, main_content).unwrap();

    let workspace = WorkspaceManager::new();
    let main_uri = Url::from_file_path(&main_path).unwrap();
    workspace.open_file(&main_uri, main_content).await.unwrap();

    // User is typing a field type inside MainMessage, has typed `o`.
    let in_progress = r#"syntax = "proto3";
package main;

import "other.proto";

message MainMessage {
    o
}
"#;
    workspace.open_file(&main_uri, in_progress).await.unwrap();

    // Cursor right after `o` (line 6, character 5).
    let labels = run_completion(
        &workspace,
        &main_uri,
        in_progress,
        Position { line: 6, character: 5 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    assert!(
        labels.iter().any(|l| l == "other."),
        "expected 'other.' (imported package name) to be in completion list inside message, got: {:?}",
        labels
    );
}

/// When the user has typed the package prefix `other.` inside a message, the
/// LSP should suggest symbols from the imported package (e.g. `OtherMessage`).
#[tokio::test]
async fn test_completion_suggests_imported_symbols_after_package_prefix() {
    use std::fs;
    use std::path::PathBuf;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let dir = tempfile::tempdir().expect("create tempdir");
    let dir_path = dir.path().to_path_buf();

    let imported_path: PathBuf = dir_path.join("other.proto");
    let imported_content = r#"syntax = "proto3";
package other;

message OtherMessage {
    string name = 1;
}
"#;
    fs::write(&imported_path, imported_content).unwrap();

    let main_path: PathBuf = dir_path.join("main.proto");
    let main_content = r#"syntax = "proto3";
package main;

import "other.proto";

message MainMessage {
    string name = 1;
}
"#;
    fs::write(&main_path, main_content).unwrap();

    let workspace = WorkspaceManager::new();
    let main_uri = Url::from_file_path(&main_path).unwrap();
    workspace.open_file(&main_uri, main_content).await.unwrap();

    // User has typed `other.` as a field type inside MainMessage.
    let in_progress = r#"syntax = "proto3";
package main;

import "other.proto";

message MainMessage {
    other.
}
"#;
    workspace.open_file(&main_uri, in_progress).await.unwrap();

    // Cursor right after `other.` (line 6, character 10).
    let labels = run_completion(
        &workspace,
        &main_uri,
        in_progress,
        Position { line: 6, character: 10 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    assert!(
        labels.iter().any(|l| l == "OtherMessage"),
        "expected 'OtherMessage' (imported symbol) to be in completion list after 'other.', got: {:?}",
        labels
    );
}

/// Transitive imports should be resolved recursively: if `main.proto` imports
/// `a.proto` and `a.proto` imports `b.proto`, symbols from `b.proto` (package
/// `b`) should be available in `main.proto` completion without `b.proto` ever
/// being opened in the editor.
#[tokio::test]
async fn test_completion_resolves_transitive_imports_recursively() {
    use std::fs;
    use std::path::PathBuf;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let dir = tempfile::tempdir().expect("create tempdir");
    let dir_path = dir.path().to_path_buf();

    // b.proto — package `b`, defines DeepMessage.
    let b_path: PathBuf = dir_path.join("b.proto");
    fs::write(
        &b_path,
        r#"syntax = "proto3";
package b;

message DeepMessage {
    string name = 1;
}
"#,
    )
    .unwrap();

    // a.proto — package `a`, imports b.proto (transitive from main's perspective).
    let a_path: PathBuf = dir_path.join("a.proto");
    fs::write(
        &a_path,
        r#"syntax = "proto3";
package a;

import "b.proto";

message AMmessage {
    string name = 1;
}
"#,
    )
    .unwrap();

    // main.proto — imports a.proto only. b.proto is a transitive import.
    let main_path: PathBuf = dir_path.join("main.proto");
    let main_content = r#"syntax = "proto3";
package main;

import "a.proto";

message MainMessage {
    string name = 1;
}
"#;
    fs::write(&main_path, main_content).unwrap();

    let workspace = WorkspaceManager::new();
    let main_uri = Url::from_file_path(&main_path).unwrap();
    workspace.open_file(&main_uri, main_content).await.unwrap();

    // Type `b.` inside MainMessage to reference a symbol from the transitive
    // import b.proto.
    let in_progress = r#"syntax = "proto3";
package main;

import "a.proto";

message MainMessage {
    b.
}
"#;
    workspace.open_file(&main_uri, in_progress).await.unwrap();

    let labels = run_completion(
        &workspace,
        &main_uri,
        in_progress,
        Position { line: 6, character: 6 },
    )
    .await;

    println!("completion labels: {:?}", labels);

    assert!(
        labels.iter().any(|l| l == "DeepMessage"),
        "expected 'DeepMessage' (symbol from transitive import b.proto) to be in completion list, got: {:?}",
        labels
    );
}
