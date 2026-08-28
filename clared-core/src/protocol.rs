use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub mod error_code {
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const INSUFFICIENT_BUDGET: i32 = -32001;
    pub const RESOURCE_OUTSIDE_ENVELOPE: i32 = -32002;
    pub const TOOL_OUTSIDE_ENVELOPE: i32 = -32003;
    pub const INVALID_CAPABILITY: i32 = -32004;
    pub const UNKNOWN_SESSION: i32 = -32005;
    pub const POLICY_VIOLATION: i32 = -32006;
    pub const MISSING_ADAPTER: i32 = -32007;
    pub const INVALID_LIFECYCLE: i32 = -32008;
    pub const IDEMPOTENCY_CONFLICT: i32 = -32009;
    pub const INVALID_DELEGATION: i32 = -32010;
}

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
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStatus {
    Proposed,
    Admitted,
    Active,
    Suspended,
    Sealing,
    Settled,
    Aborted,
    Expired,
    Revoked,
    PartiallySettled,
    RecoveryRequired,
    Reconciled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

impl TypedBudgets {
    pub fn value(&self, dimension: &str) -> Option<u64> {
        match dimension {
            "money.minor.USD.hold" => Some(self.money_minor_usd_hold),
            "money.minor.USD.capture" => Some(self.money_minor_usd_capture),
            "database.mutations.count" => Some(self.database_mutations_count),
            custom => self.custom.get(custom).copied(),
        }
    }

    pub fn add_assign(&mut self, additions: &TypedBudgets) -> Result<(), String> {
        self.money_minor_usd_hold = self
            .money_minor_usd_hold
            .checked_add(additions.money_minor_usd_hold)
            .ok_or("Hold budget overflow")?;
        self.money_minor_usd_capture = self
            .money_minor_usd_capture
            .checked_add(additions.money_minor_usd_capture)
            .ok_or("Capture budget overflow")?;
        self.database_mutations_count = self
            .database_mutations_count
            .checked_add(additions.database_mutations_count)
            .ok_or("Database mutation budget overflow")?;
        for (dimension, amount) in &additions.custom {
            let current = self.custom.get(dimension).copied().unwrap_or(0);
            self.custom.insert(
                dimension.clone(),
                current
                    .checked_add(*amount)
                    .ok_or("Custom budget overflow")?,
            );
        }
        Ok(())
    }

    pub fn deduct(&mut self, dimension: &str, amount: u64) -> Result<(), String> {
        let slot = match dimension {
            "money.minor.USD.hold" => &mut self.money_minor_usd_hold,
            "money.minor.USD.capture" => &mut self.money_minor_usd_capture,
            "database.mutations.count" => &mut self.database_mutations_count,
            custom => self
                .custom
                .get_mut(custom)
                .ok_or_else(|| format!("Budget dimension '{custom}' is not present"))?,
        };
        if *slot < amount {
            return Err(format!(
                "Budget dimension '{dimension}' has {} remaining but {amount} was requested",
                *slot
            ));
        }
        *slot -= amount;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposeParams {
    pub delegation_token: String,
    pub tenant_id: String,
    pub principal: String,
    pub agent_role: String,
    pub task_intent: String,
    #[serde(default)]
    pub target_resources: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub budgets: TypedBudgets,
    #[serde(default = "default_ttl")]
    pub ttl_ms: u64,
}

fn default_ttl() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposeResult {
    pub session_id: String,
    pub status: SessionStatus,
    pub capability_token: String,
    pub generation: u64,
    pub expires_at_ms: i64,
    pub signer_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAmendParams {
    pub session_id: String,
    pub capability_token: String,
    pub delegation_token: String,
    #[serde(default)]
    pub budget_additions: TypedBudgets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMeta {
    pub session_id: String,
    pub capability_token: String,
    pub generation: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Value,
    #[serde(rename = "_clared_meta")]
    pub meta: ToolCallMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSealParams {
    pub session_id: String,
    pub capability_token: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAbortParams {
    pub session_id: String,
    pub capability_token: String,
    pub idempotency_key: String,
    #[serde(default = "default_abort_reason")]
    pub reason: String,
}

fn default_abort_reason() -> String {
    "Explicit client abort".to_string()
}
