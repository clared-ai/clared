use crate::adapter::{AdapterRegistry, ExecutionMode, RegisteredTool};
use crate::crypto::{CapabilityClaims, CapabilitySigner};
use crate::delegation::verify_delegation_token;
use crate::policy::{CedarEngine, PolicyOutcome};
use crate::protocol::error_code::{
    IDEMPOTENCY_CONFLICT, INSUFFICIENT_BUDGET, INTERNAL_ERROR, INVALID_CAPABILITY,
    INVALID_DELEGATION, INVALID_LIFECYCLE, INVALID_PARAMS, MISSING_ADAPTER, POLICY_VIOLATION,
    RESOURCE_OUTSIDE_ENVELOPE, TOOL_OUTSIDE_ENVELOPE, UNKNOWN_SESSION,
};
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

#[derive(Debug, Clone, PartialEq)]
pub struct SessionError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl SessionError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Clared error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for SessionError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedAction {
    pub tool_name: String,
    pub arguments: Value,
    pub resource_ids: Vec<String>,
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

    pub fn propose(
        &self,
        params: IntentProposeParams,
    ) -> Result<IntentProposeResult, SessionError> {
        if !(1_000..=600_000).contains(&params.ttl_ms) {
            return Err(SessionError::new(
                INVALID_PARAMS,
                "ttl_ms must be between 1000 and 600000",
            ));
        }
        if params.allowed_tools.is_empty() {
            return Err(SessionError::new(
                INVALID_PARAMS,
                "allowed_tools must contain at least one adapted tool",
            ));
        }
        for tool in &params.allowed_tools {
            let adapter = self.adapters.get(tool).ok_or_else(|| {
                SessionError::new(
                    MISSING_ADAPTER,
                    format!("Tool '{tool}' has no registered Clared Settlement Adapter"),
                )
            })?;
            if !adapter.resource_arguments.is_empty() && params.target_resources.is_empty() {
                return Err(SessionError::new(
                    RESOURCE_OUTSIDE_ENVELOPE,
                    format!("Tool '{tool}' requires at least one target_resources scope"),
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
        )
        .map_err(|message| SessionError::new(INVALID_DELEGATION, message))?;

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
        let capability_token = self
            .signer
            .issue(&claims)
            .map_err(|message| SessionError::new(INTERNAL_ERROR, message))?;

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

    pub fn amend(&self, params: IntentAmendParams) -> Result<Value, SessionError> {
        let claims = self
            .signer
            .verify(&params.capability_token)
            .map_err(|message| SessionError::new(INVALID_CAPABILITY, message))?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&params.session_id).ok_or_else(|| {
            SessionError::new(
                UNKNOWN_SESSION,
                format!("Session '{}' not found", params.session_id),
            )
        })?;

        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err(SessionError::new(
                INVALID_LIFECYCLE,
                format!("Session cannot be amended from state {:?}", session.status),
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
        )
        .map_err(|message| SessionError::new(INVALID_DELEGATION, message))?;

        let mut next_budgets = session.remaining_budgets.clone();
        next_budgets
            .add_assign(&params.budget_additions)
            .map_err(|message| SessionError::new(INSUFFICIENT_BUDGET, message))?;
        let next_generation = session
            .generation
            .checked_add(1)
            .ok_or_else(|| SessionError::new(INTERNAL_ERROR, "Session generation overflow"))?;
        let new_claims = CapabilityClaims {
            session_id: session.session_id.clone(),
            tenant_id: session.tenant_id.clone(),
            principal: session.principal.clone(),
            generation: next_generation,
            issued_at_ms: now_ms,
            expires_at_ms: session.expires_at_ms,
            jti: format!("jti_{}", Uuid::new_v4().simple()),
        };
        let next_capability_token = self
            .signer
            .issue(&new_claims)
            .map_err(|message| SessionError::new(INTERNAL_ERROR, message))?;

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

    pub fn execute_tool(&self, params: ToolCallParams) -> Result<Value, SessionError> {
        if params.meta.idempotency_key.trim().is_empty() {
            return Err(SessionError::new(
                INVALID_PARAMS,
                "idempotency_key is required",
            ));
        }
        let claims = self
            .signer
            .verify(&params.meta.capability_token)
            .map_err(|message| SessionError::new(INVALID_CAPABILITY, message))?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let adapter = self.adapters.get(&params.name).cloned().ok_or_else(|| {
            SessionError::new(
                MISSING_ADAPTER,
                format!(
                    "Tool '{}' has no registered Settlement Adapter",
                    params.name
                ),
            )
        })?;

        let fingerprint = serde_json::to_string(&(params.name.as_str(), &params.arguments))
            .map_err(|error| {
                SessionError::new(
                    INVALID_PARAMS,
                    format!("Tool arguments are invalid: {error}"),
                )
            })?;
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&params.meta.session_id).ok_or_else(|| {
            SessionError::new(
                UNKNOWN_SESSION,
                format!("Session '{}' not found", params.meta.session_id),
            )
        })?;

        if let Err(error) =
            Self::validate_capability(session, &claims, &params.meta.capability_token, now_ms)
        {
            if now_ms >= session.expires_at_ms {
                session.status = SessionStatus::Expired;
            }
            return Err(error);
        }
        if params.meta.generation != session.generation {
            return Err(SessionError::new(
                INVALID_CAPABILITY,
                format!(
                    "Stale capability generation: presented {}, active {}",
                    params.meta.generation, session.generation
                ),
            ));
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err(SessionError::new(
                INVALID_LIFECYCLE,
                format!("Tool calls are not permitted in state {:?}", session.status),
            ));
        }
        if !session.allowed_tools.contains(&params.name) {
            return Err(SessionError::new(
                TOOL_OUTSIDE_ENVELOPE,
                format!("Tool '{}' is outside the execution envelope", params.name),
            ));
        }

        if let Some(cached) = session.tool_results.get(&params.meta.idempotency_key) {
            if cached.request_fingerprint != fingerprint {
                return Err(SessionError::new(
                    IDEMPOTENCY_CONFLICT,
                    "Idempotency key was already used for a different tool request",
                ));
            }
            return Ok(cached.response.clone());
        }

        let resource_ids =
            Self::validate_resource_scope(&adapter, &params.arguments, &session.target_resources)?;
        let policy_context = Self::policy_context(&params.arguments);
        for resource_id in &resource_ids {
            match self.policy_engine.evaluate(
                &session.principal,
                &params.name,
                resource_id,
                &policy_context,
            ) {
                PolicyOutcome::Allow => {}
                PolicyOutcome::Deny {
                    reason,
                    violating_policies,
                } => {
                    return Err(SessionError::with_data(
                        POLICY_VIOLATION,
                        format!("INVARIANT_VIOLATION: {reason}"),
                        json!({
                            "resource_id": resource_id,
                            "violating_policies": violating_policies
                        }),
                    ));
                }
            }
        }

        let charges = Self::calculate_budget_charges(&adapter, &params.arguments)?;
        for (dimension, amount) in &charges {
            let remaining = session.remaining_budgets.value(dimension).unwrap_or(0);
            if remaining < *amount {
                return Err(SessionError::with_data(
                    INSUFFICIENT_BUDGET,
                    format!(
                        "INSUFFICIENT_BUDGET: '{dimension}' requested {amount}, remaining {remaining}"
                    ),
                    json!({
                        "dimension": dimension,
                        "requested": amount,
                        "remaining": remaining,
                        "remediation_hint": "Reduce the operation or use intent/amend with a fresh delegation token"
                    }),
                ));
            }
        }
        for (dimension, amount) in charges {
            session
                .remaining_budgets
                .deduct(&dimension, amount)
                .map_err(|message| SessionError::new(INSUFFICIENT_BUDGET, message))?;
        }

        session.status = SessionStatus::Active;
        let staged_id = format!("stg_{}", Uuid::new_v4().simple());
        let response = Self::simulated_stage_response(&adapter, &staged_id);
        session.staged_actions.push(StagedAction {
            tool_name: params.name,
            arguments: params.arguments,
            resource_ids,
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

    pub fn seal(&self, params: IntentSealParams) -> Result<Value, SessionError> {
        if params.idempotency_key.trim().is_empty() {
            return Err(SessionError::new(
                INVALID_PARAMS,
                "idempotency_key is required",
            ));
        }
        let claims = self
            .signer
            .verify(&params.capability_token)
            .map_err(|message| SessionError::new(INVALID_CAPABILITY, message))?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&params.session_id).ok_or_else(|| {
            SessionError::new(
                UNKNOWN_SESSION,
                format!("Session '{}' not found", params.session_id),
            )
        })?;
        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;

        if session.status == SessionStatus::Settled {
            if session.seal_idempotency_key.as_deref() == Some(&params.idempotency_key) {
                return session.settlement_result.clone().ok_or_else(|| {
                    SessionError::new(INTERNAL_ERROR, "Settled session is missing its receipt")
                });
            }
            return Err(SessionError::new(
                IDEMPOTENCY_CONFLICT,
                "Session is already settled under a different idempotency key",
            ));
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted | SessionStatus::Active
        ) {
            return Err(SessionError::new(
                INVALID_LIFECYCLE,
                format!("Session cannot be sealed from state {:?}", session.status),
            ));
        }

        session.status = SessionStatus::Sealing;
        for action in &session.staged_actions {
            let policy_context = Self::policy_context(&action.arguments);
            for resource_id in &action.resource_ids {
                if let PolicyOutcome::Deny {
                    reason,
                    violating_policies,
                } = self.policy_engine.evaluate(
                    &session.principal,
                    &action.tool_name,
                    resource_id,
                    &policy_context,
                ) {
                    let error = SessionError::with_data(
                        POLICY_VIOLATION,
                        format!("Commit-time invariant revalidation failed: {reason}"),
                        json!({
                            "resource_id": resource_id,
                            "violating_policies": violating_policies
                        }),
                    );
                    session.status = SessionStatus::Aborted;
                    session.staged_actions.clear();
                    return Err(error);
                }
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
                    "resource_ids": action.resource_ids,
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
        let (evidence_hash, evidence_signature) = self
            .signer
            .sign_evidence(&evidence)
            .map_err(|message| SessionError::new(INTERNAL_ERROR, message))?;
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

    pub fn abort(&self, params: IntentAbortParams) -> Result<Value, SessionError> {
        if params.idempotency_key.trim().is_empty() {
            return Err(SessionError::new(
                INVALID_PARAMS,
                "idempotency_key is required",
            ));
        }
        let claims = self
            .signer
            .verify(&params.capability_token)
            .map_err(|message| SessionError::new(INVALID_CAPABILITY, message))?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&params.session_id).ok_or_else(|| {
            SessionError::new(
                UNKNOWN_SESSION,
                format!("Session '{}' not found", params.session_id),
            )
        })?;
        Self::validate_capability(session, &claims, &params.capability_token, now_ms)?;

        if session.status == SessionStatus::Aborted {
            if session.abort_idempotency_key.as_deref() == Some(&params.idempotency_key) {
                return session.abort_result.clone().ok_or_else(|| {
                    SessionError::new(INTERNAL_ERROR, "Aborted session is missing its receipt")
                });
            }
            return Err(SessionError::new(
                IDEMPOTENCY_CONFLICT,
                "Session is already aborted under a different idempotency key",
            ));
        }
        if !matches!(
            session.status,
            SessionStatus::Admitted
                | SessionStatus::Active
                | SessionStatus::Suspended
                | SessionStatus::Sealing
        ) {
            return Err(SessionError::new(
                INVALID_LIFECYCLE,
                format!("Session cannot be aborted from state {:?}", session.status),
            ));
        }

        let mut rollback_plan: Vec<&StagedAction> = session.staged_actions.iter().collect();
        rollback_plan.sort_by_key(|action| std::cmp::Reverse(action.settlement_order));
        let reverted_actions: Vec<Value> = rollback_plan
            .into_iter()
            .map(|action| {
                json!({
                    "tool": action.tool_name,
                    "adapter": action.adapter_name,
                    "resource_ids": action.resource_ids,
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
    ) -> Result<(), SessionError> {
        if presented_token != session.capability_token {
            return Err(SessionError::new(
                INVALID_CAPABILITY,
                "Capability token is not active for this session",
            ));
        }
        if claims.session_id != session.session_id
            || claims.tenant_id != session.tenant_id
            || claims.principal != session.principal
            || claims.generation != session.generation
        {
            return Err(SessionError::new(
                INVALID_CAPABILITY,
                "Capability claims do not match the active session",
            ));
        }
        if claims.expires_at_ms != session.expires_at_ms || now_ms >= session.expires_at_ms {
            return Err(SessionError::new(
                INVALID_CAPABILITY,
                "Capability and session have expired",
            ));
        }
        Ok(())
    }

    fn consume_delegation(&self, token: &str) -> Result<(), SessionError> {
        if !self.used_delegations.lock().insert(token.to_string()) {
            return Err(SessionError::new(
                INVALID_DELEGATION,
                "Delegation token has already been consumed",
            ));
        }
        Ok(())
    }

    fn validate_resource_scope(
        adapter: &RegisteredTool,
        arguments: &Value,
        target_resources: &[String],
    ) -> Result<Vec<String>, SessionError> {
        if adapter.resource_arguments.is_empty() {
            return Ok(vec!["unscoped".to_string()]);
        }

        let mut matched_resources = Vec::new();
        for resource_argument in &adapter.resource_arguments {
            let resource_value = arguments
                .get(&resource_argument.argument)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    SessionError::new(
                        RESOURCE_OUTSIDE_ENVELOPE,
                        format!(
                            "Required resource argument '{}' is missing",
                            resource_argument.argument
                        ),
                    )
                })?;
            let qualified_resource =
                format!("{}:{}", resource_argument.scope_prefix, resource_value);
            let matching_scope = target_resources
                .iter()
                .find(|scope| scope.as_str() == qualified_resource);
            let matching_scope = matching_scope.ok_or_else(|| {
                SessionError::with_data(
                    RESOURCE_OUTSIDE_ENVELOPE,
                    format!(
                        "Resource '{qualified_resource}' from '{}' is outside the execution envelope",
                        resource_argument.argument
                    ),
                    json!({ "allowed_resources": target_resources }),
                )
            })?;
            if !matched_resources.contains(matching_scope) {
                matched_resources.push(matching_scope.clone());
            }
        }

        Ok(matched_resources)
    }

    fn calculate_budget_charges(
        adapter: &RegisteredTool,
        arguments: &Value,
    ) -> Result<Vec<(String, u64)>, SessionError> {
        let mut charges: HashMap<String, u64> = HashMap::new();
        for charge in &adapter.budget_charges {
            let amount = if let Some(argument_name) = &charge.argument {
                arguments
                    .get(argument_name)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        SessionError::new(
                            INVALID_PARAMS,
                            format!(
                                "Budgeted argument '{argument_name}' must be a non-negative integer minor-unit value"
                            ),
                        )
                    })?
            } else {
                charge.constant.ok_or_else(|| {
                    SessionError::new(
                        INTERNAL_ERROR,
                        format!(
                            "Adapter budget charge '{}' has neither argument nor constant",
                            charge.dimension
                        ),
                    )
                })?
            };
            let total = charges.get(&charge.dimension).copied().unwrap_or(0);
            charges.insert(
                charge.dimension.clone(),
                total.checked_add(amount).ok_or_else(|| {
                    SessionError::new(
                        INTERNAL_ERROR,
                        format!("Adapter budget charge '{}' overflowed", charge.dimension),
                    )
                })?,
            );
        }
        Ok(charges.into_iter().collect())
    }

    fn policy_context(arguments: &Value) -> Value {
        let mut context = arguments.as_object().cloned().unwrap_or_else(Map::new);
        context.insert("clared_envelope_admitted".to_string(), Value::Bool(true));
        context.insert("has_director_approval".to_string(), Value::Bool(false));
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
    use crate::adapter::{BudgetCharge, ResourceArgument};
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
    fn untrusted_tool_arguments_cannot_assert_director_approval() {
        let (manager, _) = manager();
        let result = propose(&manager, 1);
        let error = manager
            .execute_tool(ToolCallParams {
                name: "stripe.payment_intents.create".to_string(),
                arguments: json!({
                    "amount_minor": 60000,
                    "customer_id": "cus_9918",
                    "has_director_approval": true
                }),
                meta: call_meta(&result, "forged-director-approval"),
            })
            .unwrap_err();
        assert_eq!(error.code, POLICY_VIOLATION);
    }

    #[test]
    fn every_adapter_resource_is_returned_for_policy_evaluation() {
        let adapter = RegisteredTool {
            adapter_name: "multi_resource_test".to_string(),
            mode: ExecutionMode::Mode2Mock,
            resource_arguments: vec![
                ResourceArgument {
                    argument: "order_id".to_string(),
                    scope_prefix: "order".to_string(),
                },
                ResourceArgument {
                    argument: "customer_id".to_string(),
                    scope_prefix: "customer".to_string(),
                },
            ],
            budget_charges: Vec::<BudgetCharge>::new(),
            settlement_order: 10,
            staging_strategy: "MOCK".to_string(),
            settlement_strategy: "MOCK".to_string(),
            rollback_strategy: "MOCK".to_string(),
        };
        let resources = SessionManager::validate_resource_scope(
            &adapter,
            &json!({"order_id": "ord_1042", "customer_id": "cus_9918"}),
            &[
                "order:ord_1042".to_string(),
                "customer:cus_9918".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            resources,
            vec![
                "order:ord_1042".to_string(),
                "customer:cus_9918".to_string()
            ]
        );
    }

    #[test]
    fn duplicate_adapter_budget_dimensions_are_aggregated_before_deduction() {
        let adapter = RegisteredTool {
            adapter_name: "aggregate_budget_test".to_string(),
            mode: ExecutionMode::Mode2Mock,
            resource_arguments: Vec::new(),
            budget_charges: vec![
                BudgetCharge {
                    dimension: "actions.count".to_string(),
                    argument: None,
                    constant: Some(2),
                },
                BudgetCharge {
                    dimension: "actions.count".to_string(),
                    argument: None,
                    constant: Some(3),
                },
            ],
            settlement_order: 10,
            staging_strategy: "MOCK".to_string(),
            settlement_strategy: "MOCK".to_string(),
            rollback_strategy: "MOCK".to_string(),
        };
        let charges = SessionManager::calculate_budget_charges(&adapter, &json!({})).unwrap();
        assert_eq!(charges, vec![("actions.count".to_string(), 5)]);
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
            receipt["settled_actions"][0]["resource_ids"][0],
            "order:ord_1042"
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
        let error = manager.propose(params).unwrap_err();
        assert_eq!(error.code, INVALID_DELEGATION);
    }

    #[test]
    fn terminal_errors_use_protocol_registry_codes() {
        let (manager, _) = manager();
        let result = propose(&manager, 1);
        manager
            .seal(IntentSealParams {
                session_id: result.session_id.clone(),
                capability_token: result.capability_token.clone(),
                idempotency_key: "seal-original".to_string(),
            })
            .unwrap();

        let conflict = manager
            .seal(IntentSealParams {
                session_id: result.session_id.clone(),
                capability_token: result.capability_token.clone(),
                idempotency_key: "seal-conflict".to_string(),
            })
            .unwrap_err();
        assert_eq!(conflict.code, IDEMPOTENCY_CONFLICT);

        let unknown = manager
            .seal(IntentSealParams {
                session_id: "ses_missing".to_string(),
                capability_token: result.capability_token,
                idempotency_key: "seal-missing".to_string(),
            })
            .unwrap_err();
        assert_eq!(unknown.code, UNKNOWN_SESSION);
    }
}
