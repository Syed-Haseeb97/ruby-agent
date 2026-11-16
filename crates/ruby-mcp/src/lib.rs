//! Model Context Protocol (MCP) JSON-RPC 2.0 client & manager.
//! Manages stdio and SSE MCP servers, with pre-registered Playwright sidecar.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PINNED_PLAYWRIGHT_MCP_VERSION: &str = "0.0.84";

#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP JSON-RPC error ({code}): {message}")]
    JsonRpc { code: i64, message: String },
    #[error("Process error: {0}")]
    Process(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaywrightSidecarConfig {
    pub package_version: String,
    pub browser_channel: String, // "msedge"
    pub user_data_dir: String,
    pub isolated: bool,
}

impl Default for PlaywrightSidecarConfig {
    fn default() -> Self {
        Self {
            package_version: PINNED_PLAYWRIGHT_MCP_VERSION.to_string(),
            browser_channel: "msedge".to_string(),
            user_data_dir: "D:\\projects\\ruby-agent\\workspace\\browser_profile".to_string(),
            isolated: false,
        }
    }
}

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<String>, McpError>;
}
