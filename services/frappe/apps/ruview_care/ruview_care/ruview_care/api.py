"""
Frappe whitelisted API methods — called by the FastAPI insight-api bridge.
Authentication via api_key + api_secret query params or Authorization header.

All endpoints are POST unless noted. Frappe auto-routes these to:
  POST /api/method/ruview_care.ruview_care.api.<function_name>
"""
import frappe
from frappe import _

@frappe.whitelist(allow_guest=False)
def register_deployment(deployment_id, deployment_name, location_name,
                        latitude=None, longitude=None, tenant_id=None,
                        node_count=None, tauri_version=None):
    """Upsert a deployment record. Called on every Tauri app startup."""
    existing = frappe.db.get_value("RuView Deployment", {"deployment_id": deployment_id})
    if existing:
        doc = frappe.get_doc("RuView Deployment", existing)
        doc.deployment_name = deployment_name
        doc.location_name = location_name
        if latitude:
            doc.latitude = float(latitude)
        if longitude:
            doc.longitude = float(longitude)
        if tenant_id:
            doc.tenant_id = tenant_id
        if tauri_version:
            doc.tauri_version = tauri_version
        doc.update_heartbeat(node_count=int(node_count) if node_count else None)
    else:
        doc = frappe.get_doc({
            "doctype": "RuView Deployment",
            "deployment_id": deployment_id,
            "deployment_name": deployment_name,
            "location_name": location_name,
            "latitude": float(latitude) if latitude else None,
            "longitude": float(longitude) if longitude else None,
            "tenant_id": tenant_id,
            "tauri_version": tauri_version,
            "node_count": int(node_count) if node_count else 0,
            "status": "Online",
            "active_risk_level": "low",
        })
        doc.insert(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "ok", "name": doc.name}

@frappe.whitelist(allow_guest=False)
def ingest_session(session_id, deployment_id, vital_summary=None,
                   pose_anomalies=None, duration_seconds=0, csi_snr_db=0):
    """Create a CSI Session record from the insight-api ingest endpoint."""
    import json
    dep_name = frappe.db.get_value("RuView Deployment", {"deployment_id": deployment_id})
    if not dep_name:
        frappe.throw(f"Unknown deployment_id: {deployment_id}")
    vitals = json.loads(vital_summary) if isinstance(vital_summary, str) else (vital_summary or {})
    anomalies = json.loads(pose_anomalies) if isinstance(pose_anomalies, str) else (pose_anomalies or [])
    doc = frappe.get_doc({
        "doctype": "CSI Session",
        "session_id": session_id,
        "deployment": dep_name,
        "duration_seconds": int(duration_seconds),
        "csi_snr_db": float(csi_snr_db),
        "hr_mean": vitals.get("hr_mean", 0),
        "br_mean": vitals.get("br_mean", 0),
        "presence_ratio": vitals.get("presence_ratio", 1.0) * 100,
        "pose_anomalies": ", ".join(anomalies) if anomalies else "",
        "anomaly_count": len(anomalies),
    })
    doc.insert(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "ok", "name": doc.name}

@frappe.whitelist(allow_guest=False)
def ingest_insight(session_id, report: dict):
    """Store an InsightReport from the LangGraph pipeline."""
    import json
    if isinstance(report, str):
        report = json.loads(report)
    session_name = frappe.db.get_value("CSI Session", {"session_id": session_id})
    dep_name = frappe.db.get_value("CSI Session", session_name, "deployment") if session_name else None
    risk = report.get("risk", {})
    vitals = report.get("vitals", {})
    trend = report.get("trend", {})
    doc = frappe.get_doc({
        "doctype": "Insight Report",
        "session": session_name,
        "deployment": dep_name,
        "risk_score": risk.get("composite_score", 0),
        "risk_level": risk.get("risk_level", "low"),
        "hr_classification": vitals.get("hr_classification", ""),
        "br_classification": vitals.get("br_classification", ""),
        "fall_risk_score": risk.get("fall_risk", 0) * 100,
        "trend_direction": trend.get("trend_direction", "stable"),
        "summary": report.get("summary", ""),
        "action_items": "\n".join(report.get("action_items", [])),
        "agent_trace_id": report.get("agent_trace_id", ""),
        "confidence": report.get("confidence", 0) * 100,
    })
    doc.insert(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "ok", "name": doc.name}

@frappe.whitelist(allow_guest=False)
def get_deployments_summary(tenant_id=None):
    """Aggregate stats for the Tauri Enterprise dashboard."""
    filters = {}
    if tenant_id:
        filters["tenant_id"] = tenant_id
    deps = frappe.get_all("RuView Deployment", filters=filters,
                          fields=["name", "status", "active_risk_level"])
    total = len(deps)
    online = sum(1 for d in deps if d.status == "Online")
    high_risk = sum(1 for d in deps if d.active_risk_level in ("high", "critical"))
    risk_map = {"low": 10, "moderate": 40, "high": 70, "critical": 90}
    avg = sum(risk_map.get(d.active_risk_level or "low", 10) for d in deps) / total if total else 0
    return {"total": total, "online": online, "offline": total - online,
            "high_risk": high_risk, "avg_risk_score": round(avg, 1)}
