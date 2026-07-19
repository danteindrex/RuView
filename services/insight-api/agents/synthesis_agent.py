import json, re
from langchain_openai import ChatOpenAI
from langchain_core.prompts import ChatPromptTemplate
from .state import InsightState, InsightReport, VitalsAnalysis, AnomalyAnalysis, ClinicalInterpretation, RiskAssessment, TrendAnalysis

SYNTHESIS_PROMPT = ChatPromptTemplate.from_messages([
    ("system", """You are a senior clinical AI synthesizer. Create a final clinical summary.
Return JSON:
{{"summary": "2-3 sentence clinical summary",
  "action_items": ["prioritized list of concrete actions for care team"],
  "confidence": 0.0-1.0}}
Be specific, actionable, and prioritize safety. Lead with highest-risk items."""),
    ("human", """Risk level: {risk_level} (score {score}/100, escalation={escalate})
Trend: {direction} ({sessions} sessions analyzed)
Clinical urgency: {urgency}
Primary findings: {findings}
Recommended actions: {actions}
Significant changes: {changes}"""),
])

async def synthesis_agent(state: InsightState) -> InsightState:
    v = state.get("vitals_analysis")
    a = state.get("anomaly_analysis")
    c = state.get("clinical_interpretation")
    r = state.get("risk_assessment")
    t = state.get("trend_analysis")
    session_id = state["session"].session_id
    try:
        chain = SYNTHESIS_PROMPT | ChatOpenAI(model="gpt-4o-mini", temperature=0)
        result = await chain.ainvoke({
            "risk_level": r.risk_level if r else "unknown",
            "score": r.composite_score if r else 0,
            "escalate": r.escalation_required if r else False,
            "direction": t.trend_direction if t else "unknown",
            "sessions": t.sessions_analyzed if t else 1,
            "urgency": c.urgency if c else "routine",
            "findings": "; ".join(c.primary_findings) if c else "none",
            "actions": "; ".join(c.recommended_actions) if c else "none",
            "changes": "; ".join(t.significant_changes) if t else "none",
        })
        m = re.search(r'\{.*\}', result.content, re.DOTALL)
        data = json.loads(m.group() if m else result.content)
        state["final_report"] = InsightReport(
            session_id=session_id,
            vitals=v or VitalsAnalysis(hr_classification="unknown", br_classification="unknown", hr_variability="unknown", observations=[], confidence=0),
            anomalies=a or AnomalyAnalysis(detected=[], fall_risk_score=0, mobility_score=1, severity="none"),
            clinical=c or ClinicalInterpretation(primary_findings=[], differential_considerations=[], recommended_actions=[], urgency="routine"),
            risk=r or RiskAssessment(composite_score=0, fall_risk=0, cardiovascular_risk=0, respiratory_risk=0, risk_level="low", escalation_required=False),
            trend=t or TrendAnalysis(vs_baseline={}, trend_direction="stable", sessions_analyzed=1, significant_changes=[]),
            summary=data.get("summary", "Analysis unavailable"),
            action_items=data.get("action_items", []),
            confidence=float(data.get("confidence", 0.5)),
            agent_trace_id=state["trace_id"],
        )
    except Exception as e:
        state["errors"].append(f"synthesis_agent: {e}")
    return state
