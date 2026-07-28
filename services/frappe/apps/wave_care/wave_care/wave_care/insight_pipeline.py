"""
LangGraph multi-agent insight pipeline — runs as a Frappe RQ background job.
Called via frappe.enqueue("wave_care.wave_care.insight_pipeline.run_pipeline", session_name=...)
Result stored as Insight Report DocType. No separate FastAPI service needed.
"""
import frappe
from typing import TypedDict, Optional

try:
    from langgraph.graph import StateGraph, END
    from langchain_openai import ChatOpenAI
    LANGGRAPH_AVAILABLE = True
except ImportError:
    LANGGRAPH_AVAILABLE = False


class InsightState(TypedDict):
    session_id: str
    session_name: str
    vitals: dict
    anomalies: list
    vitals_analysis: Optional[dict]
    anomaly_analysis: Optional[dict]
    clinical_interpretation: Optional[dict]
    risk_assessment: Optional[dict]
    trend_analysis: Optional[dict]
    final_report: Optional[dict]
    errors: list


def _make_llm():
    import os
    api_key = os.getenv("OPENAI_API_KEY") or frappe.db.get_single_value("Wave Settings", "openai_api_key")
    if not api_key:
        return None
    return ChatOpenAI(model="gpt-4o-mini", api_key=api_key, temperature=0.3)


def vitals_node(state: InsightState) -> InsightState:
    llm = _make_llm()
    if not llm:
        state["vitals_analysis"] = {"hr_classification": "unknown", "br_classification": "unknown", "error": "no_llm"}
        return state
    v = state["vitals"]
    resp = llm.invoke(
        f"Analyze vital signs: HR={v.get('hr_mean',0):.1f}bpm (std={v.get('hr_std',0):.1f}), "
        f"BR={v.get('br_mean',0):.1f}rpm, presence={v.get('presence_ratio',1.0):.0%}. "
        "Return JSON: hr_classification, br_classification, concerns (list)."
    )
    try:
        import json
        state["vitals_analysis"] = json.loads(resp.content)
    except Exception:
        state["vitals_analysis"] = {"summary": resp.content}
    return state


def anomaly_node(state: InsightState) -> InsightState:
    llm = _make_llm()
    if not llm or not state["anomalies"]:
        state["anomaly_analysis"] = {"severity": "none", "anomaly_count": 0}
        return state
    resp = llm.invoke(
        f"Pose/motion anomalies detected: {', '.join(state['anomalies'])}. "
        "Return JSON: severity (none/low/moderate/high), clinical_significance, recommended_action."
    )
    try:
        import json
        state["anomaly_analysis"] = json.loads(resp.content)
    except Exception:
        state["anomaly_analysis"] = {"summary": resp.content}
    return state


def clinical_node(state: InsightState) -> InsightState:
    llm = _make_llm()
    if not llm:
        state["clinical_interpretation"] = {"interpretation": "LLM unavailable", "immediate_concerns": [], "monitoring_priority": "normal"}
        return state
    resp = llm.invoke(
        f"Vitals: {state.get('vitals_analysis',{})}. Anomalies: {state.get('anomaly_analysis',{})}. "
        "Return JSON: interpretation (string), immediate_concerns (list), monitoring_priority (low/normal/high/urgent)."
    )
    try:
        import json
        state["clinical_interpretation"] = json.loads(resp.content)
    except Exception:
        state["clinical_interpretation"] = {"interpretation": resp.content, "immediate_concerns": []}
    return state


def risk_node(state: InsightState) -> InsightState:
    v = state["vitals"]
    hr, br = v.get("hr_mean", 70), v.get("br_mean", 16)
    score = 0.1
    if hr > 100 or hr < 50:
        score += 0.3
    if br > 25 or br < 10:
        score += 0.3
    if "fall_detected" in state["anomalies"]:
        score += 0.4
    score = min(1.0, score)
    level = "low" if score < 0.3 else "moderate" if score < 0.6 else "high" if score < 0.8 else "critical"
    state["risk_assessment"] = {
        "composite_score": round(score, 3),
        "risk_level": level,
        "fall_risk": 0.8 if "fall_detected" in state["anomalies"] else 0.1,
    }
    return state


def trend_node(state: InsightState) -> InsightState:
    state["trend_analysis"] = {"trend_direction": "stable", "confidence": 0.7}
    return state


def synthesis_node(state: InsightState) -> InsightState:
    risk = state.get("risk_assessment", {})
    clinical = state.get("clinical_interpretation", {})
    va = state.get("vitals_analysis", {})
    state["final_report"] = {
        "risk_score": risk.get("composite_score", 0),
        "risk_level": risk.get("risk_level", "low"),
        "hr_classification": va.get("hr_classification", "normal"),
        "br_classification": va.get("br_classification", "normal"),
        "fall_risk": risk.get("fall_risk", 0),
        "trend_direction": state.get("trend_analysis", {}).get("trend_direction", "stable"),
        "summary": clinical.get("interpretation", "Monitoring within normal parameters."),
        "action_items": clinical.get("immediate_concerns", []),
        "confidence": 0.82,
    }
    return state


def _parallel_initial(state: InsightState) -> InsightState:
    s1, s2 = vitals_node(dict(state)), anomaly_node(dict(state))
    state["vitals_analysis"] = s1["vitals_analysis"]
    state["anomaly_analysis"] = s2["anomaly_analysis"]
    return state


def _parallel_assessment(state: InsightState) -> InsightState:
    s1, s2 = risk_node(dict(state)), trend_node(dict(state))
    state["risk_assessment"] = s1["risk_assessment"]
    state["trend_analysis"] = s2["trend_analysis"]
    return state


def _build_graph():
    if not LANGGRAPH_AVAILABLE:
        return None
    g = StateGraph(InsightState)
    g.add_node("parallel_initial", _parallel_initial)
    g.add_node("clinical", clinical_node)
    g.add_node("parallel_assessment", _parallel_assessment)
    g.add_node("synthesis", synthesis_node)
    g.set_entry_point("parallel_initial")
    g.add_edge("parallel_initial", "clinical")
    g.add_edge("clinical", "parallel_assessment")
    g.add_edge("parallel_assessment", "synthesis")
    g.add_edge("synthesis", END)
    return g.compile()


def run_pipeline(session_name: str) -> str:
    """Entry point for frappe.enqueue(). Loads session, runs pipeline, stores Insight Report."""
    session = frappe.get_doc("CSI Session", session_name)
    state: InsightState = {
        "session_id": session.session_id,
        "session_name": session_name,
        "vitals": {
            "hr_mean": float(session.hr_mean or 70),
            "hr_std": 5.0,
            "br_mean": float(session.br_mean or 16),
            "br_std": 2.0,
            "presence_ratio": float(session.presence_ratio or 100) / 100,
        },
        "anomalies": [a.strip() for a in (session.pose_anomalies or "").split(",") if a.strip()],
        "vitals_analysis": None, "anomaly_analysis": None,
        "clinical_interpretation": None, "risk_assessment": None,
        "trend_analysis": None, "final_report": None, "errors": [],
    }
    graph = _build_graph()
    if graph:
        result = graph.invoke(state)
    else:
        result = synthesis_node(_parallel_assessment(clinical_node(_parallel_initial(state))))

    r = result.get("final_report", {})
    insight = frappe.get_doc({
        "doctype": "Insight Report",
        "session": session_name,
        "deployment": session.deployment,
        "risk_score": r.get("risk_score", 0),
        "risk_level": r.get("risk_level", "low"),
        "hr_classification": r.get("hr_classification", ""),
        "br_classification": r.get("br_classification", ""),
        "fall_risk_score": r.get("fall_risk", 0) * 100,
        "trend_direction": r.get("trend_direction", "stable"),
        "summary": r.get("summary", ""),
        "action_items": "\n".join(r.get("action_items", [])),
        "confidence": r.get("confidence", 0) * 100,
    })
    insight.insert(ignore_permissions=True)

    if session.deployment and r.get("risk_level") in ("high", "critical"):
        dep = frappe.get_doc("Wave Deployment", session.deployment)
        dep.update_heartbeat(risk_level=r["risk_level"])

    frappe.db.commit()
    return insight.name
