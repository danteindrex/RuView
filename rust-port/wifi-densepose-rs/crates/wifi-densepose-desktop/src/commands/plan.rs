use tauri::State;
use crate::state::AppState;
use crate::plan::PlanTier;

#[tauri::command]
pub async fn get_plan_tier(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.plan.to_string())
}

/// Gate command execution by plan tier. Returns error string if insufficient plan.
/// Usage in other commands: require_cloud_plan(&state)?;
pub fn require_cloud_plan(state: &AppState) -> Result<(), String> {
    crate::plan::require_plan(state.plan, PlanTier::Cloud)
}

pub fn require_enterprise_plan(state: &AppState) -> Result<(), String> {
    crate::plan::require_plan(state.plan, PlanTier::Enterprise)
}
