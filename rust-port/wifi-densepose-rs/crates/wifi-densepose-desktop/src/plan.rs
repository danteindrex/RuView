use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PlanTier {
    Local = 0,
    Cloud = 1,
    Enterprise = 2,
}

impl PlanTier {
    /// Derive tier from the license_type string stored in the licenses table.
    pub fn from_license_type(license_type: &str) -> Self {
        match license_type.to_lowercase().as_str() {
            "cloud" | "professional" | "pro" => PlanTier::Cloud,
            "enterprise" | "military" | "healthcare" => PlanTier::Enterprise,
            _ => PlanTier::Local,
        }
    }
}

impl std::fmt::Display for PlanTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanTier::Local => write!(f, "local"),
            PlanTier::Cloud => write!(f, "cloud"),
            PlanTier::Enterprise => write!(f, "enterprise"),
        }
    }
}

/// Returns Err if current plan does not meet minimum requirement.
pub fn require_plan(current: PlanTier, minimum: PlanTier) -> Result<(), String> {
    if current >= minimum {
        Ok(())
    } else {
        Err(format!(
            "This feature requires {} plan (current: {})",
            minimum, current
        ))
    }
}
