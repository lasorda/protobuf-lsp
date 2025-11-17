use crate::features::{
    format_document, provide_completion, provide_definition_async, provide_document_symbols,
    provide_hover, validate_proto_file, create_parse_diagnostics,
};
use crate::workspace::WorkspaceManager;
use dashmap::DashMap;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct ProtobufLanguageServer {
    client: Client,
    workspace: Arc<WorkspaceManager>,
    document_contents: Arc<DashMap<Url, String>>,
}

impl ProtobufLanguageServer {
    pub fn new(client: Client) -> Self {
        tracing::info!("Creating new ProtobufLanguageServer instance");
        let workspace = Arc::new(WorkspaceManager::new());
        tracing::info!("Workspace manager created with default resolver");
        Self {
            client,
            workspace,
            document_contents: Arc::new(DashMap::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for ProtobufLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing protobuf language server");

        // Extract additional proto directories from initialization options if provided
        tracing::info!("Checking for additional proto directories in initialization options");
        if let Some(options) = params.initialization_options {
            tracing::debug!("Initialization options: {:?}", options);
            if let Some(dirs) = options.get("additionalProtoDirs") {
                tracing::info!("Found additionalProtoDirs: {:?}", dirs);
                if let Some(dirs_array) = dirs.as_array() {
                    for dir in dirs_array {
                        if let Some(dir_str) = dir.as_str() {
                            let path_buf = std::path::PathBuf::from(dir_str);
                            tracing::info!("Adding proto directory: {}", dir_str);
                            self.workspace.add_proto_directory(path_buf);
                        }
                    }
                }
            } else {
                tracing::info!("No additionalProtoDirs found in initialization options");
            }
        } else {
            tracing::info!("No initialization options provided");
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    resolve_provider: Some(false),
                    completion_item: None,
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                }),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "protobuf-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("Protobuf language server initialized");
        self.client
            .log_message(MessageType::INFO, "Protobuf LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down protobuf language server");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;

        tracing::info!("Opening document: {}", uri);

        // Store the document content
        self.document_contents.insert(uri.clone(), content.clone());

        // Parse the file
        match self.workspace.open_file(&uri, &content).await {
            Ok(_) => {
                self.client
                    .log_message(MessageType::INFO, format!("Parsed: {}", uri))
                    .await;

                // Validate the file and publish diagnostics
                if let Err(e) = validate_proto_file(&uri, &self.workspace, &self.client).await {
                    tracing::error!("Failed to validate {}: {}", uri, e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to parse {}: {}", uri, e);
                self.client
                    .log_message(MessageType::ERROR, format!("Parse error: {}", e))
                    .await;

                // Create diagnostics for parse errors
                let diagnostics = create_parse_diagnostics(&uri, &Err(e));
                self.client.publish_diagnostics(uri, diagnostics, None).await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        if let Some(change) = params.content_changes.first() {
            let content = &change.text;

            // Update stored content
            self.document_contents.insert(uri.clone(), content.clone());

            // Re-parse the file
            match self.workspace.open_file(&uri, content).await {
                Ok(_) => {
                    // Validate the file and publish diagnostics
                    if let Err(e) = validate_proto_file(&uri, &self.workspace, &self.client).await {
                        tracing::error!("Failed to validate {}: {}", uri, e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to parse {}: {}", uri, e);

                    // Create diagnostics for parse errors
                    let diagnostics = create_parse_diagnostics(&uri, &Err(e));
                    self.client.publish_diagnostics(uri, diagnostics, None).await;
                }
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::info!("Closing document: {}", uri);

        self.document_contents.remove(&uri);
        self.workspace.close_file(&uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        tracing::debug!("Completion request: {:?}", params);
        let uri = &params.text_document_position.text_document.uri;
        let content: Option<String> = self.document_contents.get(uri).map(|s| s.clone());
        Ok(provide_completion(params, &self.workspace, content.as_deref()).await)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        tracing::debug!("Goto definition request: {:?}", params);
        let uri = &params.text_document_position_params.text_document.uri;
        if let Some(content) = self.document_contents.get(uri) {
            Ok(provide_definition_async(params, &self.workspace, Some(content.as_str())).await)
        } else {
            Ok(provide_definition_async(params, &self.workspace, None).await)
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        tracing::debug!("Hover request: {:?}", params);
        let uri = &params.text_document_position_params.text_document.uri;
        if let Some(content) = self.document_contents.get(uri) {
            Ok(provide_hover(params, &self.workspace, Some(content.as_str())))
        } else {
            Ok(provide_hover(params, &self.workspace, None))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        tracing::debug!("Document symbol request: {:?}", params);
        Ok(provide_document_symbols(params, &self.workspace))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        tracing::debug!("Formatting request: {:?}", params);

        let uri = &params.text_document.uri;
        if let Some(content) = self.document_contents.get(uri) {
            Ok(format_document(params, &content))
        } else {
            Ok(None)
        }
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        tracing::debug!("Range formatting request: {:?}", params);

        let uri = &params.text_document.uri;
        if let Some(content) = self.document_contents.get(uri) {
            // Convert DocumentRangeFormattingParams to DocumentFormattingParams
            let format_params = DocumentFormattingParams {
                text_document: params.text_document,
                options: params.options,
                work_done_progress_params: params.work_done_progress_params,
            };

            Ok(crate::features::formatting::format_range(
                format_params,
                &content,
                params.range,
            ))
        } else {
            Ok(None)
        }
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        tracing::info!("Configuration changed: {:?}", params);

        // Handle configuration changes (e.g., additional proto directories)
        if let Some(settings) = params.settings.as_object() {
            if let Some(dirs) = settings.get("additionalProtoDirs") {
                if let Some(dirs_array) = dirs.as_array() {
                    for dir in dirs_array {
                        if let Some(dir_str) = dir.as_str() {
                            self.workspace
                                .add_proto_directory(std::path::PathBuf::from(dir_str));
                        }
                    }
                }
            }
        }
    }
}
