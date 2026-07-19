import os
from langchain_openai import ChatOpenAI
from langchain_core.prompts import ChatPromptTemplate
from .state import InsightState, VitalsAnalysis

_llm = None
def get_llm():
    global _llm
    if _llm is None:
        _llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)
    return _llm

VITALS_PROMPT = ChatPromptTemplate.from_messages([
    ("system", """You are a clinical vitals analyst for a WiFi CSI sensing system.
Analyze the vital sign data and return a structured JSON assessment.
Respond ONLY with valid JSON matching this schema:
{{"hr_classification": "normal|bradycardia|tachycardia|critical",
  "br_classification": "normal|bradypnea|tachypnea",
  "hr_variability": "low|normal|high",
  "observations": ["list of clinical observations"],
  "confidence": 0.0-1.0}}

Normal ranges: HR 60-100 bpm, BR 12-20 rpm. Bradycardia <60, Tachycardia >100.
Bradypnea <12, Tachypnea >20."""),
    ("human", """HR: mean={hr_mean:.1f} std={hr_std:.1f} min={hr_min:.1f} max={hr_max:.1f} bpm
BR: mean={br_mean:.1f} rpm
Presence: {presence_ratio:.0%}
Session duration: {duration}s
CSI SNR: {snr:.1f} dB"""),
])

async def vitals_agent(state: InsightState) -> InsightState:
    s = state["session"]
    v = s.vital_summary
    try:
        chain = VITALS_PROMPT | get_llm()
        result = await chain.ainvoke({
            "hr_mean": v.get("hr_mean", 0),
            "hr_std": v.get("hr_std", 0),
            "hr_min": v.get("hr_min", v.get("hr_mean", 0) - 10),
            "hr_max": v.get("hr_max", v.get("hr_mean", 0) + 10),
            "br_mean": v.get("br_mean", 0),
            "presence_ratio": v.get("presence_ratio", 1.0),
            "duration": s.duration_seconds,
            "snr": s.csi_snr_db,
        })
        import json, re
        text = result.content
        m = re.search(r'\{.*\}', text, re.DOTALL)
        data = json.loads(m.group() if m else text)
        state["vitals_analysis"] = VitalsAnalysis(**data)
    except Exception as e:
        state["errors"].append(f"vitals_agent: {e}")
        state["vitals_analysis"] = VitalsAnalysis(
            hr_classification="unknown", br_classification="unknown",
            hr_variability="unknown", observations=[], confidence=0.0)
    return state
