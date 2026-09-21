use crate::Result;
use crate::budget::MapBudgetPlan;

pub fn format_json(plan: &MapBudgetPlan) -> Result<String> {
    Ok(serde_json::to_string_pretty(plan)?)
}
