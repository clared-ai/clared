use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request, Response,
};
use serde_json::Value;
use std::str::FromStr;

pub struct CedarEngine {
    policy_set: PolicySet,
    authorizer: Authorizer,
}

#[derive(Debug, Clone)]
pub enum PolicyOutcome {
    Allow,
    Deny {
        violating_policies: Vec<String>,
        reason: String,
    },
}

impl CedarEngine {
    /// Creates a new Cedar engine with default guardrail policies.
    pub fn new() -> Result<Self, String> {
        let default_policies = r#"
            // Allow all actions by default unless explicitly forbidden
            permit(
                principal,
                action,
                resource
            );

            // Invariant 1: Forbid high-value refund captures exceeding $500.00 without director approval
            forbid(
                principal,
                action in [
                    Action::"stripe.payment_intents.refund",
                    Action::"stripe.payment_intents.create",
                    Action::"stripe.charges.create"
                ],
                resource
            )
            when {
                context.amount_minor > 50000 && !context.has_director_approval
            };

            // Invariant 2: Forbid destructive SQL DDL operations
            forbid(
                principal,
                action in [
                    Action::"db.drop_table",
                    Action::"db.truncate",
                    Action::"postgres.drop_table"
                ],
                resource
            );
        "#;

        Self::from_policy_str(default_policies)
    }

    /// Compiles a Cedar policy set from a raw string.
    pub fn from_policy_str(policy_str: &str) -> Result<Self, String> {
        let policy_set = PolicySet::from_str(policy_str)
            .map_err(|e| format!("Cedar policy compilation error: {}", e))?;
        let authorizer = Authorizer::new();

        Ok(Self {
            policy_set,
            authorizer,
        })
    }

    /// Evaluates an action, principal, resource, and context against the compiled Cedar DAG.
    pub fn evaluate(
        &self,
        principal_id: &str,
        action_name: &str,
        resource_id: &str,
        context_json: &Value,
    ) -> PolicyOutcome {
        let principal = match EntityUid::from_str(&format!("User::\"{}\"", principal_id)) {
            Ok(value) => value,
            Err(error) => {
                return PolicyOutcome::Deny {
                    violating_policies: vec!["MALFORMED_PRINCIPAL".to_string()],
                    reason: format!("Principal identifier is invalid: {error}"),
                };
            }
        };

        let action = match EntityUid::from_str(&format!("Action::\"{}\"", action_name)) {
            Ok(value) => value,
            Err(error) => {
                return PolicyOutcome::Deny {
                    violating_policies: vec!["MALFORMED_ACTION".to_string()],
                    reason: format!("Action identifier is invalid: {error}"),
                };
            }
        };

        let resource = match EntityUid::from_str(&format!("Resource::\"{}\"", resource_id)) {
            Ok(value) => value,
            Err(error) => {
                return PolicyOutcome::Deny {
                    violating_policies: vec!["MALFORMED_RESOURCE".to_string()],
                    reason: format!("Resource identifier is invalid: {error}"),
                };
            }
        };

        // Construct Cedar context from JSON arguments
        let context = match Context::from_json_value(context_json.clone(), None) {
            Ok(value) => value,
            Err(error) => {
                return PolicyOutcome::Deny {
                    violating_policies: vec!["MALFORMED_CONTEXT".to_string()],
                    reason: format!("Policy context is invalid: {error}"),
                };
            }
        };

        let entities = Entities::empty();

        let request =
            match Request::new(Some(principal), Some(action), Some(resource), context, None) {
                Ok(req) => req,
                Err(e) => {
                    return PolicyOutcome::Deny {
                        violating_policies: vec!["MALFORMED_REQUEST".to_string()],
                        reason: format!("Failed to construct Cedar request: {}", e),
                    };
                }
            };

        let response: Response =
            self.authorizer
                .is_authorized(&request, &self.policy_set, &entities);

        match response.decision() {
            Decision::Allow => PolicyOutcome::Allow,
            Decision::Deny => {
                let diagnostics = response.diagnostics();
                let reasons: Vec<String> = diagnostics.reason().map(|r| r.to_string()).collect();

                let desc = if reasons.is_empty() {
                    "Action denied by default closed-world policy boundary".to_string()
                } else {
                    format!("Violated policy: {}", reasons.join(", "))
                };

                PolicyOutcome::Deny {
                    violating_policies: reasons,
                    reason: desc,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cedar_refund_budget_invariant() {
        let engine = CedarEngine::new().expect("Engine should compile default policies");

        // Allowed $450 refund
        let allowed_ctx = json!({
            "amount_minor": 45000,
            "has_director_approval": false
        });
        let res = engine.evaluate(
            "alice",
            "stripe.payment_intents.refund",
            "customer:cus_9918",
            &allowed_ctx,
        );
        assert!(matches!(res, PolicyOutcome::Allow));

        // Forbidden $600 refund without director approval
        let forbidden_ctx = json!({
            "amount_minor": 60000,
            "has_director_approval": false
        });
        let res2 = engine.evaluate(
            "alice",
            "stripe.payment_intents.refund",
            "customer:cus_9918",
            &forbidden_ctx,
        );
        assert!(matches!(res2, PolicyOutcome::Deny { .. }));
    }

    #[test]
    fn test_cedar_destructive_ddl_invariant() {
        let engine = CedarEngine::new().expect("Engine should compile");
        let ctx = json!({});
        let res = engine.evaluate("alice", "db.drop_table", "postgres:public", &ctx);
        assert!(matches!(res, PolicyOutcome::Deny { .. }));
    }
}
