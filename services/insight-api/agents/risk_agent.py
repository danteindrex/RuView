import json, re
from langchain_openai import ChatOpenAI
from langchain_core.prompts import ChatPromptTemplate
from .state import InsightState, RiskAssessment

RISK_PROMPT = ChatPromptTemplate.from_messages([
    ("system", """You are a healthcare risk assessment engine.
Compute a composite risk score and return JSON:
{{"composite_score": 0-100,
  "fall_risk": 0.0-1.0,
  "cardiovascular_risk": 0.0-1.0,
  "respiratory_risk": 0.0-1.0,
  "risk_level": "low|moderate|high|critical",
  "escalation_required": true|false}}
composite_score: weighted average (fall 40%, cardiovascular 35%, respiratory 25%).
escalation_required: true if composite_score >= 70 or any sub-score >= 0.85."""),
    ("human", """HR: {hr_class} | BR: {br_class}
Fall risk from anomalies: {fall_risk:.2f}
Anomaly severity: {severity}
Clinical urgency: {urgency}
Vitals confidence: {confidence:.2f}"""),
])

async def risk_agent(state: InsightState) -> InsightState:
    v = state.get("vitals_analysis")
    a = state.get("anomaly_analysis")
    c = state.get("clinical_interpretation")
    try:
        chain = RISK_PROMPT | ChatOpenAI(model="gpt-4o-mini", temperature=0)
        result = await chain.ainvoke({
            "hr_class": v.hr_classification if v else "unknown",
            "br_class": v.br_classification if v else "unknown",
            "fall_risk": a.fall_risk_score if a else 0.0,
            "severity": a.severity if a else "none",
            "urgency": c.urgency if c else "routine",
            "confidence": v.confidence if v else 0.5,
        })
        m = re.search(r'\{.*\}', result.content, re.DOTALL)
        data = json.loads(m.group() if m else result.content)
        state["risk_assessment"] = RiskAssessment(**data)
    except Exception as e:
        state["errors"].append(f"risk_agent: {e}")
        state["risk_assessment"] = RiskAssessment(
            composite_score=0, fall_risk=0, cardiovascular_risk=0,
            respiratory_risk=0, risk_level="low", escalation_required=False)
    return state
