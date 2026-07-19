import os, json, re
from langchain_openai import ChatOpenAI
from langchain_core.prompts import ChatPromptTemplate
from .state import InsightState, AnomalyAnalysis

ANOMALY_PROMPT = ChatPromptTemplate.from_messages([
    ("system", """You are a fall risk and mobility analyst for WiFi CSI sensing.
Analyze detected pose anomalies and return JSON:
{{"detected": ["list of confirmed anomalies"],
  "fall_risk_score": 0.0-1.0,
  "mobility_score": 0.0-1.0,
  "severity": "none|mild|moderate|severe"}}
fall_risk_score: 0=no risk, 1=imminent. mobility_score: 1=fully mobile, 0=immobile."""),
    ("human", "Detected anomalies: {anomalies}\nDuration: {duration}s\nPresence ratio: {presence}"),
])

async def anomaly_agent(state: InsightState) -> InsightState:
    s = state["session"]
    try:
        chain = ANOMALY_PROMPT | ChatOpenAI(model="gpt-4o-mini", temperature=0)
        result = await chain.ainvoke({
            "anomalies": ", ".join(s.pose_anomalies) if s.pose_anomalies else "none",
            "duration": s.duration_seconds,
            "presence": s.vital_summary.get("presence_ratio", 1.0),
        })
        m = re.search(r'\{.*\}', result.content, re.DOTALL)
        data = json.loads(m.group() if m else result.content)
        state["anomaly_analysis"] = AnomalyAnalysis(**data)
    except Exception as e:
        state["errors"].append(f"anomaly_agent: {e}")
        state["anomaly_analysis"] = AnomalyAnalysis(
            detected=[], fall_risk_score=0.0, mobility_score=1.0, severity="none")
    return state
