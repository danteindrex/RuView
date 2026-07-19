"""LangGraph multi-agent insights pipeline."""
import uuid
from langgraph.graph import StateGraph, END
from .state import InsightState, SessionContext
from .vitals_agent import vitals_agent
from .anomaly_agent import anomaly_agent
from .clinical_agent import clinical_agent
from .risk_agent import risk_agent
from .trend_agent import trend_agent
from .synthesis_agent import synthesis_agent


def should_escalate(state: InsightState) -> str:
    r = state.get("risk_assessment")
    if r and r.escalation_required:
        return "urgent_pathway"
    return "standard_pathway"


async def parallel_initial(state: InsightState) -> InsightState:
    """Run vitals + anomaly in parallel (both are independent)."""
    import asyncio
    results = await asyncio.gather(
        vitals_agent(dict(state)),
        anomaly_agent(dict(state)),
        return_exceptions=True,
    )
    for r in results:
        if isinstance(r, dict):
            state["vitals_analysis"] = r.get("vitals_analysis") or state.get("vitals_analysis")
            state["anomaly_analysis"] = r.get("anomaly_analysis") or state.get("anomaly_analysis")
            if r.get("errors"):
                state["errors"].extend(r["errors"])
    return state


async def parallel_assessment(state: InsightState) -> InsightState:
    """Run risk + trend in parallel after clinical."""
    import asyncio
    results = await asyncio.gather(
        risk_agent(dict(state)),
        trend_agent(dict(state)),
        return_exceptions=True,
    )
    for r in results:
        if isinstance(r, dict):
            state["risk_assessment"] = r.get("risk_assessment") or state.get("risk_assessment")
            state["trend_analysis"] = r.get("trend_analysis") or state.get("trend_analysis")
            if r.get("errors"):
                state["errors"].extend(r["errors"])
    return state


def build_insight_graph() -> StateGraph:
    g = StateGraph(InsightState)
    g.add_node("parallel_initial", parallel_initial)
    g.add_node("clinical", clinical_agent)
    g.add_node("parallel_assessment", parallel_assessment)
    g.add_node("synthesis", synthesis_agent)
    g.set_entry_point("parallel_initial")
    g.add_edge("parallel_initial", "clinical")
    g.add_edge("clinical", "parallel_assessment")
    g.add_edge("parallel_assessment", "synthesis")
    g.add_edge("synthesis", END)
    return g.compile()


_graph = None
def get_graph():
    global _graph
    if _graph is None:
        _graph = build_insight_graph()
    return _graph


async def run_insight_pipeline(ctx: SessionContext) -> "InsightState":
    trace_id = str(uuid.uuid4())
    initial_state: InsightState = {
        "session": ctx,
        "vitals_analysis": None,
        "anomaly_analysis": None,
        "clinical_interpretation": None,
        "risk_assessment": None,
        "trend_analysis": None,
        "final_report": None,
        "errors": [],
        "trace_id": trace_id,
    }
    return await get_graph().ainvoke(initial_state)
