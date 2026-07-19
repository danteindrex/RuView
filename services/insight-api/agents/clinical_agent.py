import json, re
from langchain_openai import ChatOpenAI
from langchain_core.prompts import ChatPromptTemplate
from .state import InsightState, ClinicalInterpretation

CLINICAL_PROMPT = ChatPromptTemplate.from_messages([
    ("system", """You are a clinical decision support system for WiFi-based patient monitoring.
Given vitals and anomaly analysis, provide clinical interpretation as JSON:
{{"primary_findings": ["key clinical findings"],
  "differential_considerations": ["possible conditions to evaluate"],
  "recommended_actions": ["specific actionable recommendations for care team"],
  "urgency": "routine|priority|urgent|emergency"}}
Be concise and evidence-based. Flag safety concerns prominently."""),
    ("human", """Vitals: HR={hr_class}, BR={br_class}, variability={hr_var}
Observations: {observations}
Anomalies: severity={severity}, fall_risk={fall_risk:.2f}
Detected: {detected}"""),
])

async def clinical_agent(state: InsightState) -> InsightState:
    v = state.get("vitals_analysis")
    a = state.get("anomaly_analysis")
    if not v or not a:
        state["errors"].append("clinical_agent: missing upstream results")
        return state
    try:
        chain = CLINICAL_PROMPT | ChatOpenAI(model="gpt-4o-mini", temperature=0)
        result = await chain.ainvoke({
            "hr_class": v.hr_classification, "br_class": v.br_classification,
            "hr_var": v.hr_variability, "observations": "; ".join(v.observations),
            "severity": a.severity, "fall_risk": a.fall_risk_score,
            "detected": ", ".join(a.detected) or "none",
        })
        m = re.search(r'\{.*\}', result.content, re.DOTALL)
        data = json.loads(m.group() if m else result.content)
        state["clinical_interpretation"] = ClinicalInterpretation(**data)
    except Exception as e:
        state["errors"].append(f"clinical_agent: {e}")
        state["clinical_interpretation"] = ClinicalInterpretation(
            primary_findings=[], differential_considerations=[],
            recommended_actions=[], urgency="routine")
    return state
