use std::collections::HashMap;
use parking_lot::RwLock;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::policy::{PolicyDecision, PolicyEngine};

#[derive(Debug, Clone)]
pub struct EscrowItem {
    pub step_index: usize,
    pub tool_name: String,
    pub arguments: Value,
    pub virt_token: Option<String>,
}

pub struct EpochSession {
    pub epoch_id: String,
    pub tenant_id: String,
    pub staged_items: RwLock<Vec<EscrowItem>>,
    pub token_map: RwLock<HashMap<String, String>>, // virt_token -> real_token
    pub is_aborted: RwLock<bool>,
}

impl EpochSession {
    pub fn new(tenant_id: &str) -> Self {
        Self {
            epoch_id: format!("ep_{}", Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_string(),
            staged_items: RwLock::new(Vec::new()),
            token_map: RwLock::new(HashMap::new()),
            is_aborted: RwLock::new(false),
        }
    }

    pub fn intercept_tool_call(
        &self,
        tool_name: &str,
        arguments: &Value,
        policy: &PolicyEngine,
    ) -> Result<Value, String> {
        if *self.is_aborted.read() {
            return Err("Epoch has been aborted due to a previous invariant breach.".to_string());
        }

        // 1. Evaluate against Policy Decision Graph (<0.2ms)
        match policy.evaluate(tool_name, arguments, &self.tenant_id) {
            PolicyDecision::Deny { reason } => {
                *self.is_aborted.write() = true;
                self.staged_items.write().clear();
                return Err(reason);
            }
            PolicyDecision::Allow => {}
        }

        // 2. Classify execution mode and handle escrow
        let mut staged = self.staged_items.write();
        let step_index = staged.len() + 1;

        if tool_name.contains("charge") || tool_name.contains("payment") {
            // Two-Phase Reserve + Synthetic Token
            let virt_token = format!("virt_ch_{}", &Uuid::new_v4().simple().to_string()[..8]);
            staged.push(EscrowItem {
                step_index,
                tool_name: tool_name.to_string(),
                arguments: arguments.clone(),
                virt_token: Some(virt_token.clone()),
            });

            // Return synthetic mock
            Ok(json!({
                "id": virt_token,
                "status": "authorized",
                "_clared_escrow": "HELD_IN_RESERVE"
            }))
        } else {
            // Buffer standard tool call
            staged.push(EscrowItem {
                step_index,
                tool_name: tool_name.to_string(),
                arguments: arguments.clone(),
                virt_token: None,
            });

            Ok(json!({
                "status": "STAGED_IN_ESCROW",
                "step": step_index
            }))
        }
    }

    pub fn seal_and_flush(&self) -> Result<Value, String> {
        if *self.is_aborted.read() {
            return Err("Cannot seal an aborted epoch.".to_string());
        }

        let staged = self.staged_items.read();
        let count = staged.len();

        // In real execution: phase 1 dispatch, phase 2 token rewrite, phase 3 commit
        Ok(json!({
            "status": "ATOMIC_SETTLEMENT_SUCCESS",
            "epoch_id": self.epoch_id,
            "settled_actions_count": count
        }))
    }
}
