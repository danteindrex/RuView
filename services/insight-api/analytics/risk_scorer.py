"""Composite risk scoring with weighted clinical factors."""
from dataclasses import dataclass

WEIGHTS = {"fall": 0.40, "cardiovascular": 0.35, "respiratory": 0.25}

HR_RISK = {"normal": 0.05, "bradycardia": 0.45, "tachycardia": 0.55, "critical": 0.95, "unknown": 0.3}
BR_RISK = {"normal": 0.05, "bradypnea": 0.50, "tachypnea": 0.45, "unknown": 0.3}

@dataclass
class RiskFactors:
    hr_class: str
    br_class: str
    fall_risk_from_anomalies: float
    anomaly_severity: str     # none/mild/moderate/severe
    presence_ratio: float

SEVERITY_MULTIPLIER = {"none": 1.0, "mild": 1.2, "moderate": 1.5, "severe": 2.0}

def compute_risk(f: RiskFactors) -> dict:
    cv_risk = HR_RISK.get(f.hr_class, 0.3)
    resp_risk = BR_RISK.get(f.br_class, 0.3)
    fall_risk = min(1.0, f.fall_risk_from_anomalies * SEVERITY_MULTIPLIER.get(f.anomaly_severity, 1.0))
    composite = (
        WEIGHTS["fall"] * fall_risk +
        WEIGHTS["cardiovascular"] * cv_risk +
        WEIGHTS["respiratory"] * resp_risk
    ) * 100
    # Presence correction: low presence may mean detection is unreliable
    if f.presence_ratio < 0.3:
        composite *= 0.7
    level = (
        "critical" if composite >= 80 else
        "high" if composite >= 60 else
        "moderate" if composite >= 35 else "low"
    )
    return {
        "composite_score": round(composite, 1),
        "fall_risk": round(fall_risk, 3),
        "cardiovascular_risk": round(cv_risk, 3),
        "respiratory_risk": round(resp_risk, 3),
        "risk_level": level,
        "escalation_required": composite >= 70 or fall_risk >= 0.85,
    }
