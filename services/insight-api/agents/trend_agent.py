import os, json
from .state import InsightState, TrendAnalysis

_session_cache: dict[str, list[dict]] = {}

async def trend_agent(state: InsightState) -> InsightState:
    s = state["session"]
    v = s.vital_summary
    key = "global"
    history = _session_cache.get(key, [])
    current = {"hr_mean": v.get("hr_mean", 0), "br_mean": v.get("br_mean", 0),
                "anomaly_count": len(s.pose_anomalies)}
    changes = []
    direction = "stable"
    baseline = {}
    if history:
        baseline = history[-1]
        hr_delta = current["hr_mean"] - baseline.get("hr_mean", current["hr_mean"])
        br_delta = current["br_mean"] - baseline.get("br_mean", current["br_mean"])
        anomaly_delta = current["anomaly_count"] - baseline.get("anomaly_count", 0)
        baseline = {"hr_delta": hr_delta, "br_delta": br_delta, "anomaly_delta": anomaly_delta}
        if abs(hr_delta) > 10: changes.append(f"HR shifted {hr_delta:+.1f} bpm vs last session")
        if abs(br_delta) > 3:  changes.append(f"BR shifted {br_delta:+.1f} rpm vs last session")
        if anomaly_delta > 0:  changes.append(f"{anomaly_delta} more anomalies than last session")
        worsening = (hr_delta > 15 or br_delta > 5 or anomaly_delta > 2)
        improving  = (hr_delta < -5 and br_delta < -2 and anomaly_delta <= 0)
        direction = "deteriorating" if worsening else ("improving" if improving else "stable")
    history.append(current)
    _session_cache[key] = history[-20:]  # keep last 20 sessions in memory
    state["trend_analysis"] = TrendAnalysis(
        vs_baseline=baseline, trend_direction=direction,
        sessions_analyzed=len(history), significant_changes=changes)
    return state
