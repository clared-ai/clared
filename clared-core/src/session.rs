use crate::adapter::{AdapterRegistry, ExecutionMode, RegisteredTool};
use crate::crypto::{CapabilityClaims, CapabilitySigner};
use crate::delegation::verify_delegation_token;
use crate::policy::{CedarEngine, PolicyOutcome};
use crate::protocol::{
    IntentAbortParams, IntentAmendParams, IntentProposeParams, IntentProposeResult,
    IntentSealParams, SessionStatus, ToolCallParams, TypedBudgets,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

type ToolError = (i32, String, Option<Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedAction {
    pub tool_name: String,
    pub arguments: Value,
    pub resource_id: String,
    pub adapter_name: String,
    pub mode: ExecutionMode,
    pub settlement_order: u32,
    pub settlement_strategy: String,
    pub rollback_strategy: String,
    pub staged_id: String,
    pub status: String,
}

struct CachedToolResult {
    request_fingerprint: String,
    response: Value,
}

pub struct ActiveSession {
    session_id: String,
    tenant_id: String,
    principal: String,
    agent_role: String,
    task_intent: String,
    target_resources: Vec<String>,
    allowed_tools: Vec<String>,
    remaining_budgets: TypedBudgets,
    capability_token: String,
    generation: u64,
    expires_at_ms: i64,
    status: SessionStatus,
    staged_actions: Vec<StagedAction>,
    tool_results: HashMap<String, CachedToolResult>,
    seal_idempotency_key: Option<String>,
    settlement_result: Option<Value>,
    abort_idempotency_key: Option<String>,
    abort_result: Option<Value>,
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
    policy_engine: Arc<CedarEngine>,
    signer: Arc<CapabilitySigner>,
    delegation_secret: Vec<u8>,
    used_delegations: Arc<Mutex<HashSet<String>>>,
    adapters: Arc<AdapterRegistry>,
}

impl SessionManager {
    pub fn new(
        policy_engine: Arc<CedarEngine>,
        signer: Arc<CapabilitySigner>,
        delegation_secret: Vec<u8>,
        adapters: Arc<AdapterRegistry>,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            policy_engine,
            signer,
            delegation_secret,
            used_delegations: Arc::new(Mutex::new(HashSet::new())),
            adapters,
        }
    }

    pub fn propose(&self, params: IntentProposeParams) -> Result<IntentProposeResult, String> {
        if !(1_000..=600_000).contains(&params.ttl_ms) {
            return Err("ttl_ms must be between 1000 and 600000".to_string());
        }
        if params.allowed_tools.is_empty() {
            return Err("allowed_tools must contain at least one adapted tool".to_string());
        }
        for tool in &params.allowed_tools {
            let adapter = self.adapters.get(tool).ok_or_else(|| {
                format!("Tool '{tool}' has no registered Clared Settlement Adapter")
            })?;
            if !adapter.resource_arguments.is_empty() && params.target_resources.is_empty() {
                return Err(format!(
                    "Tool '{tool}' requires at least one target_resources scope"
                ));
            }
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        verify_delegation_token(
            &self.delegation_secret,
            &params.delegation_token,
            &params.tenant_id,
            &params.principal,
            &params.agent_role,
            &params.task_intent,
            now_ms,
        )?;

        let session_id = format!("ses_{}", Uuid::new_v4().simple());
        let expires_at_ms = now_ms + params.ttl_ms as i64;
        let claims = CapabilityClaims {
            session_id: session_id.clone(),
            tenant_id: params.tenant_id.clone(),
            principal: params.principal.clone(),
            generation: 1,
            issued_at_ms: now_ms,
            expires_at_ms,
            jti: format!("jti_{}", Uuid::new_v4().simple()),
        };
        let capability_token = self.signer.issue(&claims)?;

        let session = ActiveSession {
            session_id: session_id.clone(),
            tenant_id: params.tenant_id,
            principal: params.principal,
            agent_role: params.agent_role,
            task_intent: params.task_intent,
            target_resources: params.target_resources,
            allowed_tools: params.allowed_tools,
            remaining_budgets: params.budgets,
            capability_token: capability_token.clone(),
            generation: 1,
            expires_at_ms,
            status: SessionStatus::Admitted,
            staged_actions: Vec::new(),
            tool_results: HashMap::new(),
            seal_idempotency_key: None,
            settlement_result: None,
            abort_idempotency_key: None,
            abort_result: None,
        };
        self.consume_delegation(&params.delegation_token)?;
        self.sessions.write().insert(session_id.clone(), session);

        Ok(IntentProposeResult {
            session_id,
            status: SessionStatus::Admitted,
            capability_token,
            generation: 1,
            expires_at_ms,
            signer_public_key: self.signer.public_key_base64(),
        })
    }

    pub fn amend(&self, params: IntentAmendParams) -> Result<Value, String> {
        let claims = self.signer.verify(&params.capability_token)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(&params.session_id)
            .ok_or_else(|| format!("Session '{}' not found", params.session_id))?;

        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err(format!(
                "Session cannot be amended from state {:?}",
                session.status
            ));
        }
        verify_delegation_token(
            &self.delegation_secret,
            &params.delegation_token,
            &session.tenant_id,
            &session.principal,
            &session.agent_role,
            &session.task_intent,
            now_ms,
        )?;

        let mut next_budgets = session.remaining_budgets.clone();
        next_budgets.add_assign(&params.budget_additions)?;
        let next_generation = session
            .generation
            .checked_add(1)
            .ok_or("Session generation overflow")?;
        let new_claims = CapabilityClaims {
            session_id: session.session_id.clone(),
            tenant_id: session.tenant_id.clone(),
            principal: session.principal.clone(),
            generation: next_generation,
            issued_at_ms: now_ms,
            expires_at_ms: session.expires_at_ms,
            jti: format!("jti_{}", Uuid::new_v4().simple()),
        };
        let next_capability_token = self.signer.issue(&new_claims)?;

        self.consume_delegation(&params.delegation_token)?;
        session.status = SessionStatus::Suspended;
        session.remaining_budgets = next_budgets;
        session.generation = next_generation;
        session.capability_token = next_capability_token;
        session.status = SessionStatus::Active;

        Ok(json!({
            "session_id": session.session_id,
            "status": session.status,
            "capability_token": session.capability_token,
            "generation": session.generation,
            "expires_at_ms": session.expires_at_ms,
            "remaining_budgets": session.remaining_budgets,
        }))
    }

    pub fn execute_tool(&self, params: ToolCallParams) -> Result<Value, ToolError> {
        if params.meta.idempotency_key.trim().is_empty() {
            return Err((-32602, "idempotency_key is required".to_string(), None));
        }
        let claims = self
            .signer
            .verify(&params.meta.capability_token)
            .map_err(|message| (-32004, message, None))?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let adapter = self.adapters.get(&params.name).cloned().ok_or_else(|| {
            (
                -32007,
                format!(
                    "Tool '{}' has no registered Settlement Adapter",
                    params.name
                ),
                None,
            )
        })?;

        let fingerprint = serde_json::to_string(&(params.name.as_str(), &params.arguments))
            .map_err(|error| (-32602, format!("Tool arguments are invalid: {error}"), None))?;
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&params.meta.session_id).ok_or_else(|| {
            (
                -32005,
                format!("Session '{}' not found", params.meta.session_id),
                None,
            )
        })?;

        if let Err(message) =
            Self::validate_capability(session, &claims, &params.meta.capability_token, now_ms)
        {
            if now_ms >= session.expires_at_ms {
                session.status = SessionStatus::Expired;
            }
            return Err((-32004, message, None));
        }
        if params.meta.generation != session.generation {
            return Err((
                -32004,
                format!(
                    "Stale capability generation: presented {}, active {}",
                    params.meta.generation, session.generation
                ),
                None,
            ));
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err((
                -32008,
                format!("Tool calls are not permitted in state {:?}", session.status),
                None,
            ));
        }
        if !session.allowed_tools.contains(&params.name) {
            return Err((
                -32003,
                format!("Tool '{}' is outside the execution envelope", params.name),
                None,
            ));
        }

        if let Some(cached) = session.tool_results.get(&params.meta.idempotency_key) {
            if cached.request_fingerprint != fingerprint {
                return Err((
                    -32009,
                    "Idempotency key was already used for a different tool request".to_string(),
                    None,
                ));
            }
            return Ok(cached.response.clone());
        }

        let resource_id =
            Self::validate_resource_scope(&adapter, &params.arguments, &session.target_resources)?;
        let policy_context = Self::policy_context(&params.arguments);
        match self.policy_engine.evaluate(
            &session.principal,
            &params.name,
            &resource_id,
            &policy_context,
        ) {
            PolicyOutcome::Allow => {}
            PolicyOutcome::Deny {
                reason,
                violating_policies,
            } => {
                return Err((
                    -32006,
                    format!("INVARIANT_VIOLATION: {reason}"),
                    Some(json!({ "violating_policies": violating_policies })),
                ));
            }
        }

        let charges = Self::calculate_budget_charges(&adapter, &params.arguments)?;
        for (dimension, amount) in &charges {
            let remaining = session.remaining_budgets.value(dimension).unwrap_or(0);
            if remaining < *amount {
                return Err((
                    -32001,
                    format!(
                        "INSUFFICIENT_BUDGET: '{dimension}' requested {amount}, remaining {remaining}"
                    ),
                    Some(json!({
                        "dimension": dimension,
                        "requested": amount,
                        "remaining": remaining,
                        "remediation_hint": "Reduce the operation or use intent/amend with a fresh delegation token"
                    })),
                ));
            }
        }
        for (dimension, amount) in charges {
            session
                .remaining_budgets
                .deduct(&dimension, amount)
                .map_err(|message| (-32001, message, None))?;
        }

        session.status = SessionStatus::Active;
        let staged_id = format!("stg_{}", Uuid::new_v4().simple());
        let response = Self::simulated_stage_response(&adapter, &staged_id);
        session.staged_actions.push(StagedAction {
            tool_name: params.name,
            arguments: params.arguments,
            resource_id,
            adapter_name: adapter.adapter_name,
            mode: adapter.mode,
            settlement_order: adapter.settlement_order,
            settlement_strategy: adapter.settlement_strategy,
            rollback_strategy: adapter.rollback_strategy,
            staged_id,
            status: "SIMULATED_STAGED".to_string(),
        });
        session.tool_results.insert(
            params.meta.idempotency_key,
            CachedToolResult {
                request_fingerprint: fingerprint,
                response: response.clone(),
            },
        );

        Ok(response)
    }

    pub fn seal(&self, params: IntentSealParams) -> Result<Value, String> {
        if params.idempotency_key.trim().is_empty() {
            return Err("idempotency_key is required".to_string());
        }
        let claims = self.signer.verify(&params.capability_token)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(&params.session_id)
            .ok_or_else(|| format!("Session '{}' not found", params.session_id))?;
        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;

        if session.status == SessionStatus::Settled {
            if session.seal_idempotency_key.as_deref() == Some(&params.idempotency_key) {
                return session
                    .settlement_result
                    .clone()
                    .ok_or_else(|| "Settled session is missing its receipt".to_string());
            }
            return Err("Session is already settled under a different idempotency key".to_string());
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err(format!(
                "Session cannot be sealed from state {:?}",
                session.status
            ));
        }

        session.status = SessionStatus::Sealing;
        for action in &session.staged_actions {
            let policy_context = Self::policy_context(&action.arguments);
            if let PolicyOutcome::Deny { reason, .. } = self.policy_engine.evaluate(
                &session.principal,
                &action.tool_name,
                &action.resource_id,
                &policy_context,
            ) {
                session.status = SessionStatus::Aborted;
                session.staged_actions.clear();
                return Err(format!(
                    "Commit-time invariant revalidation failed: {reason}"
                ));
            }
        }

        let mut settlement_plan: Vec<&StagedAction> = session.staged_actions.iter().collect();
        settlement_plan.sort_by_key(|action| action.settlement_order);
        let settled_actions: Vec<Value> = settlement_plan
            .into_iter()
            .map(|action| {
                let status = match action.mode {
                    ExecutionMode::Mode1Sql => "SIMULATED_COMMITTED",
                    ExecutionMode::Mode3Reservation => "SIMULATED_CAPTURED",
                    ExecutionMode::EgressSink => "SIMULATED_DISPATCHED",
                    ExecutionMode::Mode2Mock => "SIMULATED_MATERIALIZED",
                    ExecutionMode::Mode4Checkpoint => "SIMULATED_CHECKPOINTED",
                };
                json!({
                    "tool": action.tool_name,
                    "adapter": action.adapter_name,
                    "staged_id": action.staged_id,
                    "status": status,
                    "mode": action.mode,
                    "settlement_order": action.settlement_order,
                    "settlement_strategy": action.settlement_strategy,
                })
            })
            .collect();
        let evidence = json!({
            "session_id": session.session_id,
            "tenant_id": session.tenant_id,
            "principal": session.principal,
            "generation": session.generation,
            "execution_backend": "in_memory_simulator",
            "settled_actions": settled_actions,
        });
        let (evidence_hash, evidence_signature) = self.signer.sign_evidence(&evidence)?;
        let result = json!({
            "session_id": session.session_id,
            "status": "SETTLED",
            "execution_backend": "in_memory_simulator",
            "evidence": evidence,
            "evidence_hash": evidence_hash,
            "evidence_signature": evidence_signature,
            "signer_public_key": self.signer.public_key_base64(),
            "settled_actions": settled_actions,
        });

        session.status = SessionStatus::Settled;
        session.seal_idempotency_key = Some(params.idempotency_key);
        session.settlement_result = Some(result.clone());
        Ok(result)
    }

    pub fn abort(&self, params: IntentAbortParams) -> Result<Value, String> {
        if params.idempotency_key.trim().is_empty() {
            return Err("idempotency_key is required".to_string());
        }
        let claims = self.signer.verify(&params.capability_token)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(&params.session_id)
            .ok_or_else(|| format!("Session '{}' not found", params.session_id))?;
        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;

        if session.status == SessionStatus::Aborted {
            if session.abort_idempotency_key.as_deref() == Some(&params.idempotency_key) {
                return session
                    .abort_result
                    .clone()
                    .ok_or_else(|| "Aborted session is missing its receipt".to_string());
            }
            return Err("Session is already aborted under a different idempotency key".to_string());
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted
                | SessionStatus::Active
                | SessionStatus::Suspended
                | SessionStatus::Sealing
        ) {
            return Err(format!(
                "Session cannot be aborted from state {:?}",
                session.status
            ));
        }

        let mut rollback_plan: Vec<&StagedAction> = session.staged_actions.iter().collect();
        rollback_plan.sort_by(|left, right| right.settlement_order.cmp(&left.settlement_order));
        let reverted_actions: Vec<Value> = rollback_plan
            .into_iter()
            .map(|action| {
                json!({
                    "tool": action.tool_name,
                    "adapter": action.adapter_name,
                    "staged_id": action.staged_id,
                    "status": "SIMULATED_REVERTED",
                    "rollback_strategy": action.rollback_strategy,
                })
            })
            .collect();
        let reverted_actions_count = reverted_actions.len();
        session.staged_actions.clear();
        session.status = SessionStatus::Aborted;
        let result = json!({
            "session_id": session.session_id,
            "status": "ABORTED",
            "execution_backend": "in_memory_simulator",
            "reason": params.reason,
            "reverted_actions": reverted_actions,
            "reverted_actions_count": reverted_actions_count,
            "escaped_side_effects": 0,
        });
        session.abort_idempotency_key = Some(params.idempotency_key);
        session.abort_result = Some(result.clone());
        Ok(result)
    }

    fn validate_capability(
        session: &ActiveSession,
        claims: &CapabilityClaims,
        presented_token: &str,
        now_ms: i64,
    ) -> Result<(), String> {
        if presented_token != session.capability_token {
            return Err("Capability token is not active for this session".to_string());
        }
        if claims.session_id != session.session_id
            || claims.tenant_id != session.tenant_id
            || claims.principal != session.principal
            || claims.generation != session.generation
        {
            return Err("Capability claims do not match the active session".to_string());
        }
        if claims.expires_at_ms != session.expires_at_ms || now_ms >= session.expires_at_ms {
            return Err("Capability and session have expired".to_string());
        }
        Ok(())
    }

    fn consume_delegation(&self, token: &str) -> Result<(), String> {
        if !self.used_delegations.lock().insert(token.to_string()) {
            return Err("Delegation token has already been consumed".to_string());
        }
        Ok(())
    }

    fn validate_resource_scope(
        adapter: &RegisteredTool,
        arguments: &Value,
        target_resources: &[String],
    ) -> Result<String, ToolError> {
        if adapter.resource_arguments.is_empty() {
            return Ok(target_resources
                .first()
                .cloned()
                .unwrap_or_else(|| "unscoped".to_string()));
        }

        let mut first_match = None;
        for resource_argument in &adapter.resource_arguments {
            let resource_value = arguments
                .get(&resource_argument.argument)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    (
                        -32002,
                        format!(
                            "Required resource argument '{}' is missing",
                            resource_argument.argument
                        ),
                        None,
                    )
                })?;
            let qualified_resource =
                format!("{}:{}", resource_argument.scope_prefix, resource_value);
            let matching_scope = target_resources.iter().find(|scope| {
                scope.as_str() == resource_value || scope.as_str() == qualified_resource
            });
            let matching_scope = matching_scope.ok_or_else(|| {
                (
                    -32002,
                    format!(
                        "Resource '{qualified_resource}' from '{}' is outside the execution envelope",
                        resource_argument.argument
                    ),
                    Some(json!({ "allowed_resources": target_resources })),
                )
            })?;
            if first_match.is_none() {
                first_match = Some(matching_scope.clone());
            }
        }

        Ok(first_match.unwrap_or_else(|| "unscoped".to_string()))
    }

    fn calculate_budget_charges(
        adapter: &RegisteredTool,
        arguments: &Value,
    ) -> Result<Vec<(String, u64)>, ToolError> {
        let mut charges = Vec::new();
        for charge in &adapter.budget_charges {
            let amount = if let Some(argument_name) = &charge.argument {
                arguments
                    .get(argument_name)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        (
                            -32602,
                            format!(
                                "Budgeted argument '{argument_name}' must be a non-negative integer minor-unit value"
                            ),
                            None,
                        )
                    })?
            } else {
                charge.constant.ok_or_else(|| {
                    (
                        -32603,
                        format!(
                            "Adapter budget charge '{}' has neither argument nor constant",
                            charge.dimension
                        ),
                        None,
                    )
                })?
            };
            charges.push((charge.dimension.clone(), amount));
        }
        Ok(charges)
    }

    fn policy_context(arguments: &Value) -> Value {
        let mut context = arguments.as_object().cloned().unwrap_or_else(Map::new);
        context
            .entry("has_director_approval".to_string())
            .or_insert(Value::Bool(false));
        Value::Object(context)
    }

    fn simulated_stage_response(adapter: &RegisteredTool, staged_id: &str) -> Value {
        let (status, message) = match adapter.mode {
            ExecutionMode::Mode1Sql => (
                "SIMULATED_STAGED_TX",
                "Database mutation recorded by the in-memory transaction simulator.",
            ),
            ExecutionMode::Mode3Reservation => (
                "SIMULATED_AUTH_HOLD",
                "Provider authorization hold recorded by the in-memory adapter simulator.",
            ),
            ExecutionMode::EgressSink => (
                "SIMULATED_BUFFERED_EGRESS",
                "Notification recorded by the in-memory egress simulator.",
            ),
            ExecutionMode::Mode2Mock => (
                "SIMULATED_STAGED_OBJECT",
                "Object recorded by the in-memory overlay simulator.",
            ),
            ExecutionMode::Mode4Checkpoint => (
                "SIMULATED_CHECKPOINT",
                "Checkpoint recorded by the in-memory simulator.",
            ),
        };
        json!({
            "staged_id": staged_id,
            "status": status,
            "execution_backend": "in_memory_simulator",
            "staging_strategy": adapter.staging_strategy,
            "message": message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CapabilitySigner;
    use crate::delegation::issue_delegation_token;
    use crate::protocol::ToolCallMeta;
    use serde_json::json;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn manager() -> (SessionManager, Arc<CapabilitySigner>) {
        let signer = Arc::new(CapabilitySigner::from_seed([5_u8; 32]));
        (
            SessionManager::new(
                Arc::new(CedarEngine::new().unwrap()),
                signer.clone(),
                SECRET.to_vec(),
                Arc::new(AdapterRegistry::built_in().unwrap()),
            ),
            signer,
        )
    }

    fn propose(manager: &SessionManager, db_budget: u64) -> IntentProposeResult {
        let now = chrono::Utc::now().timestamp_millis();
        let token = issue_delegation_token(
            SECRET,
            "acme",
            "alice",
            "checkout",
            "authorize_order",
            now + 60_000,
        )
        .unwrap();
        manager
            .propose(IntentProposeParams {
                delegation_token: token,
                tenant_id: "acme".to_string(),
                principal: "alice".to_string(),
                agent_role: "checkout".to_string(),
                task_intent: "authorize_order".to_string(),
                target_resources: vec![
                    "customer:cus_9918".to_string(),
                    "order:ord_1042".to_string(),
                ],
                allowed_tools: vec![
                    "stripe.payment_intents.create".to_string(),
                    "postgres.orders.update".to_string(),
                    "twilio.messages.create".to_string(),
                ],
                budgets: TypedBudgets {
                    money_minor_usd_hold: 50_000,
                    money_minor_usd_capture: 50_000,
                    database_mutations_count: db_budget,
                    custom: HashMap::from([("external_notifications.count".to_string(), 1)]),
                },
                ttl_ms: 30_000,
            })
            .unwrap()
    }

    fn call_meta(result: &IntentProposeResult, key: &str) -> ToolCallMeta {
        ToolCallMeta {
            session_id: result.session_id.clone(),
            capability_token: result.capability_token.clone(),
            generation: result.generation,
            idempotency_key: key.to_string(),
        }
    }

    #[test]
    fn forged_expired_and_out_of_scope_calls_are_rejected() {
        let (manager, _) = manager();
        let result = propose(&manager, 1);

        let mut forged = call_meta(&result, "call-forged");
        forged.capability_token = "forged".to_string();
        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "cus_9918"}),
                meta: forged,
            })
            .is_err());

        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "cus_outside"}),
                meta: call_meta(&result, "call-scope"),
            })
            .is_err());

        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "ord_1042"}),
                meta: call_meta(&result, "call-wrong-resource-type"),
            })
            .is_err());

        manager
            .sessions
            .write()
            .get_mut(&result.session_id)
            .unwrap()
            .expires_at_ms = chrono::Utc::now().timestamp_millis() - 1;
        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "cus_9918"}),
                meta: call_meta(&result, "call-expired"),
            })
            .is_err());
    }

    #[test]
    fn budgets_and_tool_idempotency_are_enforced() {
        let (manager, _) = manager();
        let result = propose(&manager, 1);
        let params = ToolCallParams {
            name: "postgres.orders.update".to_string(),
            arguments: json!({"order_id": "ord_1042", "status": "authorized"}),
            meta: call_meta(&result, "db-call-1"),
        };
        let first = manager.execute_tool(params).unwrap();
        let replay = manager
            .execute_tool(ToolCallParams {
                name: "postgres.orders.update".to_string(),
                arguments: json!({"order_id": "ord_1042", "status": "authorized"}),
                meta: call_meta(&result, "db-call-1"),
            })
            .unwrap();
        assert_eq!(first, replay);
        assert!(manager
            .execute_tool(ToolCallParams {
                name: "postgres.orders.update".to_string(),
                arguments: json!({"order_id": "ord_1042", "status": "second"}),
                meta: call_meta(&result, "db-call-2"),
            })
            .is_err());
    }

    #[test]
    fn settlement_is_signed_idempotent_and_terminal() {
        let (manager, signer) = manager();
        let result = propose(&manager, 1);
        manager
            .execute_tool(ToolCallParams {
                name: "twilio.messages.create".to_string(),
                arguments: json!({"to": "+15550192834", "body": "done"}),
                meta: call_meta(&result, "twilio-call-1"),
            })
            .unwrap();
        manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 45000, "customer_id": "cus_9918"}),
                meta: call_meta(&result, "stripe-call-1"),
            })
            .unwrap();
        manager
            .execute_tool(ToolCallParams {
                name: "postgres.orders.update".to_string(),
                arguments: json!({"order_id": "ord_1042", "status": "authorized"}),
                meta: call_meta(&result, "postgres-call-1"),
            })
            .unwrap();
        let seal_params = IntentSealParams {
            session_id: result.session_id.clone(),
            capability_token: result.capability_token.clone(),
            idempotency_key: "seal-1".to_string(),
        };
        let receipt = manager.seal(seal_params.clone()).unwrap();
        assert_eq!(receipt, manager.seal(seal_params).unwrap());
        CapabilitySigner::verify_evidence(
            receipt["signer_public_key"].as_str().unwrap(),
            &receipt["evidence"],
            receipt["evidence_signature"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["signer_public_key"], signer.public_key_base64());
        assert_eq!(
            receipt["settled_actions"][0]["tool"],
            "postgres.orders.update"
        );
        assert_eq!(
            receipt["settled_actions"][1]["tool"],
            "stripe.payment_intents.create"
        );
        assert_eq!(
            receipt["settled_actions"][2]["tool"],
            "twilio.messages.create"
        );

        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "cus_9918"}),
                meta: call_meta(&result, "after-settle"),
            })
            .is_err());
        assert!(manager
            .abort(IntentAbortParams {
                session_id: result.session_id,
                capability_token: result.capability_token,
                idempotency_key: "abort-after-settle".to_string(),
                reason: "must fail".to_string(),
            })
            .is_err());
    }

    #[test]
    fn amendment_fences_the_previous_capability() {
        let (manager, _) = manager();
        let result = propose(&manager, 1);
        let now = chrono::Utc::now().timestamp_millis();
        let delegation_token = issue_delegation_token(
            SECRET,
            "acme",
            "alice",
            "checkout",
            "authorize_order",
            now + 60_000,
        )
        .unwrap();
        let amended = manager
            .amend(IntentAmendParams {
                session_id: result.session_id.clone(),
                capability_token: result.capability_token.clone(),
                delegation_token,
                budget_additions: TypedBudgets {
                    money_minor_usd_hold: 10_000,
                    money_minor_usd_capture: 10_000,
                    database_mutations_count: 0,
                    custom: HashMap::new(),
                },
            })
            .unwrap();
        assert_eq!(amended["generation"], 2);
        assert!(manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({"amount_minor": 1000, "customer_id": "cus_9918"}),
                meta: call_meta(&result, "stale-call"),
            })
            .is_err());
    }

    #[test]
    fn delegation_tokens_are_single_use() {
        let (manager, _) = manager();
        let now = chrono::Utc::now().timestamp_millis();
        let delegation_token = issue_delegation_token(
            SECRET,
            "acme",
            "alice",
            "checkout",
            "authorize_order",
            now + 60_000,
        )
        .unwrap();
        let params = IntentProposeParams {
            delegation_token,
            tenant_id: "acme".to_string(),
            principal: "alice".to_string(),
            agent_role: "checkout".to_string(),
            task_intent: "authorize_order".to_string(),
            target_resources: vec!["order:ord_1042".to_string()],
            allowed_tools: vec!["postgres.orders.update".to_string()],
            budgets: TypedBudgets {
                database_mutations_count: 1,
                ..TypedBudgets::default()
            },
            ttl_ms: 30_000,
        };
        manager.propose(params.clone()).unwrap();
        assert!(manager.propose(params).is_err());
    }
}
