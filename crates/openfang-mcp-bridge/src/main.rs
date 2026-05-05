//! Stdio entrypoint for the OpenFang MCP bridge.
//!
//! In the spike scaffold this binary runs the bridge with no dispatcher
//! attached — it serves only the stub `ping` tool. The real launch path
//! (parent agent spawns this binary as a child, hands it an identity-scoped
//! dispatcher over IPC) is the subject of ANAI-31.
//!
//! Usage (spike validation):
//!
//! ```text
//! npx @modelcontextprotocol/inspector cargo run -p openfang-mcp-bridge
//! ```

use anyhow::Result;
use openfang_mcp_bridge::Bridge;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Tracing goes to stderr — stdout is the MCP transport, do not pollute it.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("openfang_mcp_bridge=info,rmcp=warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("openfang-mcp-bridge starting (spike scaffold, no dispatcher)");

    let service = Bridge::new(None)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!(error = ?e, "bridge serve failed"))?;

    service.waiting().await?;
    Ok(())
}
