mod features;
mod parser;
mod server;
mod workspace;

use server::ProtobufLanguageServer;
use tower_lsp::{LspService, Server};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "protobuf_lsp=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting Protobuf Language Server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| ProtobufLanguageServer::new(client));

    Server::new(stdin, stdout, socket).serve(service).await;

    tracing::info!("Protobuf Language Server stopped");
}
