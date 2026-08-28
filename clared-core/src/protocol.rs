use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    PROPOSED,
    ADMITTED,
    ACTIVE,
    SEALING,
    SETTLED,
    ABORTED,
    PARTIALLY_SETTLED,
    RECOVERY_REQUIRED,
    RECONCILED,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedBudgets {
    #[serde(default, rename = "money.minor.USD.hold")]
    pub money_minor_usd_hold: u64,
    #[serde(default, rename = "money.minor.USD.capture")]
    pub money_minor_usd_capture: u64,
    #[serde(default, rename = "database.mutations.count")]
    pub database_mutations_count: u64,
    #[serde(default, flatten)]
    pub custom: HashMap<String, u64>,
}

impl Default for TypedBudgets {
    fn default() -> Self {
        Self {
            money_minor_usd_hold: 50000,
            money_minor_usd_capture: 50000,
            database_mutations_count: 10,
            custom: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposeParams {
    pub tenant_id: String,
    pub principal: String,
    pub agent_role: String,
    pub task_intent: String,
    #[serde(default)]
    pub target_resources: Vec<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub budgets: TypedBudgets,
    #[serde(default = "default_ttl")]
    pub ttl_ms: u64,
}

fn default_ttl() -> u64 {
    30000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposeResult {
    pub session_id: String,
    pub status: SessionStatus,
    pub capability_token: String,
    pub generation: u64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMeta {
    pub session_id: String,
    pub capability_token: String,
    #[serde(default = "default_gen")]
    pub generation: u64,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

fn default_gen() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Value,
    #[serde(default, rename = "_dtbe_meta")]
    pub meta: Option<ToolCallMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSealParams {
    pub session_id: String,
    pub capability_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAbortParams {
    pub session_id: String,
    pub capability_token: String,
    #[serde(default = "default_abort_reason")]
    pub reason: String,
}

fn default_abort_reason() -> String {
    "Explicit client abort".to_string()
}
