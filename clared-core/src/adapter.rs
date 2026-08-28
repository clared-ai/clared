use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const STRIPE_ADAPTER: &str = include_str!("../../adapters/stripe_payment_intent.yaml");
const POSTGRES_ADAPTER: &str = include_str!("../../adapters/postgres_orders.yaml");
const TWILIO_ADAPTER: &str = include_str!("../../adapters/twilio_sms.yaml");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionMode {
    #[serde(rename = "MODE_1_SQL")]
    Mode1Sql,
    #[serde(rename = "MODE_2_MOCK")]
    Mode2Mock,
    #[serde(rename = "MODE_3_RESERVATION")]
    Mode3Reservation,
    #[serde(rename = "MODE_4_CHECKPOINT")]
    Mode4Checkpoint,
    #[serde(rename = "EGRESS_SINK")]
    EgressSink,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BudgetCharge {
    pub dimension: String,
    #[serde(default)]
    pub argument: Option<String>,
    #[serde(default)]
    pub constant: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceArgument {
    pub argument: String,
    pub scope_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterTarget {
    pub tool_name: String,
    #[serde(default)]
    pub resource_arguments: Vec<ResourceArgument>,
    #[serde(default)]
    pub budget_charges: Vec<BudgetCharge>,
    pub settlement_order: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct AdapterMetadata {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExecutionHook {
    strategy: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AdapterExecution {
    staging: ExecutionHook,
    settlement: ExecutionHook,
    rollback: ExecutionHook,
}

#[derive(Debug, Clone, Deserialize)]
struct SettlementAdapter {
    version: String,
    metadata: AdapterMetadata,
    targets: Vec<AdapterTarget>,
    mode: ExecutionMode,
    execution: AdapterExecution,
}

#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub adapter_name: String,
    pub mode: ExecutionMode,
    pub resource_arguments: Vec<ResourceArgument>,
    pub budget_charges: Vec<BudgetCharge>,
    pub settlement_order: u32,
    pub staging_strategy: String,
    pub settlement_strategy: String,
    pub rollback_strategy: String,
}

#[derive(Debug, Clone)]
pub struct AdapterRegistry {
    tools: HashMap<String, RegisteredTool>,
}

impl AdapterRegistry {
    pub fn built_in() -> Result<Self, String> {
        Self::from_yaml_documents(&[STRIPE_ADAPTER, POSTGRES_ADAPTER, TWILIO_ADAPTER])
    }

    pub fn from_yaml_documents(documents: &[&str]) -> Result<Self, String> {
        let mut tools = HashMap::new();
        for document in documents {
            let adapter: SettlementAdapter = serde_yaml::from_str(document)
                .map_err(|error| format!("Settlement adapter YAML is invalid: {error}"))?;
            if adapter.version != "clared.dev/settlement-adapter/v0alpha1" {
                return Err(format!(
                    "Adapter '{}' uses unsupported version '{}'",
                    adapter.metadata.name, adapter.version
                ));
            }
            for (phase, strategy) in [
                ("staging", &adapter.execution.staging.strategy),
                ("settlement", &adapter.execution.settlement.strategy),
                ("rollback", &adapter.execution.rollback.strategy),
            ] {
                if strategy.trim().is_empty() {
                    return Err(format!(
                        "Adapter '{}' has an empty {phase} strategy",
                        adapter.metadata.name
                    ));
                }
            }

            for target in adapter.targets {
                let tool_name = target.tool_name.clone();
                for resource in &target.resource_arguments {
                    if resource.argument.trim().is_empty()
                        || resource.scope_prefix.trim().is_empty()
                    {
                        return Err(format!(
                            "Adapter '{}' has an invalid resource argument for tool '{tool_name}'",
                            adapter.metadata.name
                        ));
                    }
                }
                let registered = RegisteredTool {
                    adapter_name: adapter.metadata.name.clone(),
                    mode: adapter.mode.clone(),
                    resource_arguments: target.resource_arguments,
                    budget_charges: target.budget_charges,
                    settlement_order: target.settlement_order,
                    staging_strategy: adapter.execution.staging.strategy.clone(),
                    settlement_strategy: adapter.execution.settlement.strategy.clone(),
                    rollback_strategy: adapter.execution.rollback.strategy.clone(),
                };
                if tools.insert(tool_name.clone(), registered).is_some() {
                    return Err(format!("Duplicate adapter target for tool '{tool_name}'"));
                }
            }
        }

        Ok(Self { tools })
    }

    pub fn get(&self, tool_name: &str) -> Option<&RegisteredTool> {
        self.tools.get(tool_name)
    }

    pub fn contains(&self, tool_name: &str) -> bool {
        self.tools.contains_key(tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_registry_maps_tools_to_declared_modes() {
        let registry = AdapterRegistry::built_in().unwrap();
        assert_eq!(
            registry.get("postgres.orders.update").unwrap().mode,
            ExecutionMode::Mode1Sql
        );
        assert_eq!(
            registry.get("stripe.payment_intents.create").unwrap().mode,
            ExecutionMode::Mode3Reservation
        );
        assert_eq!(
            registry.get("twilio.messages.create").unwrap().mode,
            ExecutionMode::EgressSink
        );
        assert!(
            registry
                .get("twilio.messages.create")
                .unwrap()
                .settlement_order
                > registry
                    .get("stripe.payment_intents.create")
                    .unwrap()
                    .settlement_order
        );
    }
}
