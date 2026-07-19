from typing import TypedDict, Optional
from pydantic import BaseModel

class SessionContext(BaseModel):
    session_id: str
    vital_summary: dict           # {hr_mean, hr_std, br_mean, br_std, hr_min, hr_max, presence_ratio}
    pose_anomalies: list[str]     # ["fall_detected", "unusual_gait", "stillness_prolonged"]
    duration_seconds: int
    csi_snr_db: float = 0.0       # signal quality
    csi_vision_image_b64: Optional[str] = None  # from feature/latentcsi-image-gen

class VitalsAnalysis(BaseModel):
    hr_classification: str        # "normal" | "bradycardia" | "tachycardia" | "critical"
    br_classification: str        # "normal" | "bradypnea" | "tachypnea"
    hr_variability: str           # "low" | "normal" | "high"
    observations: list[str]
    confidence: float

class AnomalyAnalysis(BaseModel):
    detected: list[str]
    fall_risk_score: float        # 0.0-1.0
    mobility_score: float         # 0.0-1.0 (1 = fully mobile)
    severity: str                 # "none" | "mild" | "moderate" | "severe"

class ClinicalInterpretation(BaseModel):
    primary_findings: list[str]
    differential_considerations: list[str]
    recommended_actions: list[str]
    urgency: str                  # "routine" | "priority" | "urgent" | "emergency"

class RiskAssessment(BaseModel):
    composite_score: float        # 0-100
    fall_risk: float
    cardiovascular_risk: float
    respiratory_risk: float
    risk_level: str               # "low" | "moderate" | "high" | "critical"
    escalation_required: bool

class TrendAnalysis(BaseModel):
    vs_baseline: dict             # {hr_delta, br_delta, anomaly_delta}
    trend_direction: str          # "improving" | "stable" | "deteriorating"
    sessions_analyzed: int
    significant_changes: list[str]

class InsightReport(BaseModel):
    session_id: str
    vitals: VitalsAnalysis
    anomalies: AnomalyAnalysis
    clinical: ClinicalInterpretation
    risk: RiskAssessment
    trend: TrendAnalysis
    summary: str
    action_items: list[str]       # prioritized list for care team
    confidence: float
    agent_trace_id: str

class InsightState(TypedDict):
    session: SessionContext
    vitals_analysis: Optional[VitalsAnalysis]
    anomaly_analysis: Optional[AnomalyAnalysis]
    clinical_interpretation: Optional[ClinicalInterpretation]
    risk_assessment: Optional[RiskAssessment]
    trend_analysis: Optional[TrendAnalysis]
    final_report: Optional[InsightReport]
    errors: list
    trace_id: str
