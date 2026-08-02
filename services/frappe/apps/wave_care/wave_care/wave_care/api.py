"""
Frappe whitelisted API methods — called by the FastAPI insight-api bridge.
Authentication via api_key + api_secret query params or Authorization header.

All endpoints are POST unless noted. Frappe auto-routes these to:
  POST /api/method/wave_care.wave_care.api.<function_name>
"""
import frappe
from frappe import _

@frappe.whitelist(allow_guest=False)
def register_deployment(deployment_id, deployment_name, location_name,
                        latitude=None, longitude=None, tenant_id=None,
                        node_count=None, tauri_version=None):
    """Upsert a deployment record. Called on every Tauri app startup."""
    existing = frappe.db.get_value("Wave Deployment", {"deployment_id": deployment_id})
    if existing:
        doc = frappe.get_doc("Wave Deployment", existing)
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
            "doctype": "Wave Deployment",
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
def bind_patient(node_ip, patient, deployment_id=None):
    """Bind a sensing node to an ERPNext Healthcare Patient so its vitals are
    written to that patient's clinical record. Reuses the ERPNext `Patient`
    DocType — we do not reinvent patient identity."""
    if not frappe.db.exists("Patient", patient):
        frappe.throw(_("Patient {0} not found").format(patient))
    node = frappe.db.get_value("Sensing Node", {"node_ip": node_ip})
    if not node:
        frappe.throw(_("Sensing Node {0} not found").format(node_ip))
    doc = frappe.get_doc("Sensing Node", node)
    doc.patient = patient
    doc.save(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "ok", "node": node, "patient": patient}


@frappe.whitelist(allow_guest=False)
def ingest_vitals(patient, heart_rate=None, respiratory_rate=None,
                  temperature=None, source="Wave WiFi-CSI"):
    """Write a vitals reading into ERPNext Healthcare's **Vital Signs** DocType
    for a Patient — the standard clinical record. Reuses the ERPNext Healthcare
    module instead of a bespoke store. Only real measured values are written."""
    if not frappe.db.exists("Patient", patient):
        frappe.throw(_("Patient {0} not found").format(patient))
    doc = frappe.get_doc({
        "doctype": "Vital Signs",
        "patient": patient,
        "signs_date": frappe.utils.today(),
        "signs_time": frappe.utils.nowtime(),
    })
    # Only set vitals that were actually measured (> 0) — never a fabricated 0.
    if heart_rate and float(heart_rate) > 0:
        doc.pulse = float(heart_rate)
    if respiratory_rate and float(respiratory_rate) > 0:
        doc.respiratory_rate = float(respiratory_rate)
    if temperature and float(temperature) > 0:
        doc.temperature = float(temperature)
    doc.insert(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "ok", "vital_signs": doc.name, "patient": patient}


@frappe.whitelist(allow_guest=False)
def ingest_session(session_id, deployment_id, vital_summary=None,
                   pose_anomalies=None, duration_seconds=0, csi_snr_db=0):
    """Create a CSI Session record from the insight-api ingest endpoint."""
    import json
    dep_name = frappe.db.get_value("Wave Deployment", {"deployment_id": deployment_id})
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
    deps = frappe.get_all("Wave Deployment", filters=filters,
                          fields=["name", "status", "active_risk_level"])
    total = len(deps)
    online = sum(1 for d in deps if d.status == "Online")
    high_risk = sum(1 for d in deps if d.active_risk_level in ("high", "critical"))
    risk_map = {"low": 10, "moderate": 40, "high": 70, "critical": 90}
    avg = sum(risk_map.get(d.active_risk_level or "low", 10) for d in deps) / total if total else 0
    return {"total": total, "online": online, "offline": total - online,
            "high_risk": high_risk, "avg_risk_score": round(avg, 1)}

@frappe.whitelist(allow_guest=False)
def ingest_csi_session(session_id: str, deployment_id: str, vital_summary=None,
                        pose_anomalies=None, duration_seconds=0, csi_snr_db=0,
                        hmac_signature=None):
    """
    Primary ingest endpoint called directly by the Tauri app (no FastAPI needed).
    Stores as CSI Session DocType then auto-enqueues the LangGraph pipeline.
    """
    import json
    import hmac as hmac_mod
    import hashlib
    import os

    configured_key = os.getenv("INGEST_SIGNING_KEY") or \
        frappe.db.get_single_value("Wave Settings", "ingest_signing_key")
    if configured_key and hmac_signature:
        payload_str = f"{session_id}:{deployment_id}:{duration_seconds}"
        expected = hmac_mod.new(configured_key.encode(), payload_str.encode(), hashlib.sha256).hexdigest()
        if not hmac_mod.compare_digest(expected, hmac_signature):
            frappe.throw("Invalid HMAC signature", frappe.AuthenticationError)

    vitals = json.loads(vital_summary) if isinstance(vital_summary, str) else (vital_summary or {})
    anomalies = json.loads(pose_anomalies) if isinstance(pose_anomalies, str) else (pose_anomalies or [])

    dep_name = frappe.db.get_value("Wave Deployment", {"deployment_id": deployment_id})
    if not dep_name:
        frappe.throw(f"Unknown deployment_id: {deployment_id}")

    session = frappe.get_doc({
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
    session.insert(ignore_permissions=True)
    frappe.db.commit()

    frappe.enqueue(
        "wave_care.wave_care.insight_pipeline.run_pipeline",
        session_name=session.name,
        queue="long",
        timeout=300,
    )
    return {"status": "ok", "session_name": session.name, "insight_queued": True}


@frappe.whitelist(allow_guest=False)
def run_insight(session_name: str):
    """
    Manually trigger (or re-trigger) the LangGraph pipeline for a CSI Session.
    Returns job info. Tauri polls get_insight_by_session_id for the result.
    """
    job = frappe.enqueue(
        "wave_care.wave_care.insight_pipeline.run_pipeline",
        session_name=session_name,
        queue="long",
        timeout=300,
    )
    return {"job_id": job.id, "session_name": session_name}


@frappe.whitelist(allow_guest=False)
def get_insight_by_session_id(session_id: str):
    """Look up the latest Insight Report for a given session_id (from Tauri)."""
    session_name = frappe.db.get_value("CSI Session", {"session_id": session_id}, "name")
    if not session_name:
        return None
    reports = frappe.get_all(
        "Insight Report",
        filters={"session": session_name},
        fields=["name", "risk_score", "risk_level", "hr_classification", "br_classification",
                "fall_risk_score", "trend_direction", "summary", "action_items", "confidence",
                "creation"],
        order_by="creation desc",
        limit=1,
    )
    return reports[0] if reports else None


@frappe.whitelist(allow_guest=False)
def get_analytics_trends(deployment_id=None, limit=20):
    """HR/BR trend data for the last N sessions. Optionally filtered by deployment."""
    filters = {}
    if deployment_id:
        dep_name = frappe.db.get_value("Wave Deployment", {"deployment_id": deployment_id})
        if dep_name:
            filters["deployment"] = dep_name
    sessions = frappe.get_all(
        "CSI Session",
        filters=filters,
        fields=["session_id", "session_time", "hr_mean", "br_mean", "presence_ratio"],
        order_by="session_time desc",
        limit=int(limit),
    )
    # Pair with risk level from latest Insight Report per session
    trends = []
    for s in reversed(sessions):
        session_name = frappe.db.get_value("CSI Session", {"session_id": s.session_id})
        report = frappe.db.get_value(
            "Insight Report", {"session": session_name},
            ["risk_level", "risk_score"], as_dict=True
        ) if session_name else {}
        trends.append({
            "timestamp": str(s.session_time),
            "hr_mean": float(s.hr_mean or 0),
            "br_mean": float(s.br_mean or 0),
            "presence_ratio": float(s.presence_ratio or 0),
            "risk_level": (report or {}).get("risk_level", "low"),
            "risk_score": float((report or {}).get("risk_score", 0)),
        })
    return {"trends": trends}


@frappe.whitelist(allow_guest=False)
def get_risk_distribution(deployment_id=None):
    """Count of Insight Reports by risk level (for bar chart)."""
    filters = {}
    if deployment_id:
        dep_name = frappe.db.get_value("Wave Deployment", {"deployment_id": deployment_id})
        if dep_name:
            filters["deployment"] = dep_name
    reports = frappe.get_all("Insight Report", filters=filters, fields=["risk_level"])
    distribution = {"low": 0, "moderate": 0, "high": 0, "critical": 0}
    for r in reports:
        level = r.risk_level or "low"
        distribution[level] = distribution.get(level, 0) + 1
    total = sum(distribution.values())
    return {"distribution": distribution, "total": total}


# ─── Patient Flow API ──────────────────────────────────────────────────────

@frappe.whitelist(allow_guest=False)
def register_patient_arrival(patient_id: str, node_id: str) -> dict:
    """Create a Patient Visit and return the patient_token for the sensing server."""
    visit = frappe.new_doc("Patient Visit")
    visit.patient = patient_id
    visit.enrollment_node_id = node_id
    visit.insert(ignore_permissions=True)
    frappe.db.commit()
    return {
        "patient_token": visit.patient_token,
        "visit_name": visit.name,
        "check_in_time": str(visit.check_in_time),
    }


@frappe.whitelist(allow_guest=False)
def store_enrollment(patient_token: str, embedding_vector: str, confidence_score: float = 0.0) -> dict:
    """Store CSI enrollment fingerprint for a patient."""
    visit = frappe.get_value(
        "Patient Visit", {"patient_token": patient_token, "status": "Active"}, "name"
    )
    if not visit:
        frappe.throw(f"No active visit for token {patient_token}")
    enrollment = frappe.new_doc("CSI Enrollment")
    enrollment.patient_visit = visit
    enrollment.patient_token = patient_token
    enrollment.node_id = frappe.get_value("Patient Visit", visit, "enrollment_node_id") or ""
    enrollment.enrollment_timestamp = frappe.utils.now_datetime()
    enrollment.embedding_vector = embedding_vector
    import json
    try:
        vec = json.loads(embedding_vector)
        enrollment.embedding_dimension = len(vec)
    except Exception:
        enrollment.embedding_dimension = 0
    enrollment.confidence_score = confidence_score
    enrollment.status = "Complete"
    enrollment.insert(ignore_permissions=True)
    frappe.db.set_value("Patient Visit", visit, "enrollment_embedding", embedding_vector)
    frappe.db.set_value("Patient Visit", visit, "enrollment_confidence", confidence_score)
    frappe.db.commit()
    return {"status": "stored", "enrollment_name": enrollment.name}


@frappe.whitelist(allow_guest=False)
def zone_event(patient_token: str, zone_id: str, event_type: str, timestamp: str = None, vital_hr: float = None, vital_br: float = None) -> dict:
    """Record a patient entering or exiting a zone.
    event_type: 'enter' or 'exit'
    """
    import frappe.utils
    ts = frappe.utils.get_datetime(timestamp) if timestamp else frappe.utils.now_datetime()
    visit_name = frappe.get_value(
        "Patient Visit", {"patient_token": patient_token, "status": "Active"}, "name"
    )
    if not visit_name:
        return {"status": "no_active_visit"}
    zone_name_val = frappe.get_value("Zone", {"zone_id": zone_id}, "name")
    if not zone_name_val:
        return {"status": "zone_not_found", "zone_id": zone_id}
    if event_type == "enter":
        dwell = frappe.new_doc("Patient Zone Dwell")
        dwell.patient_visit = visit_name
        dwell.patient_token = patient_token
        dwell.zone = zone_name_val
        dwell.time_in = ts
        if vital_hr:
            dwell.vital_hr_mean = vital_hr
        if vital_br:
            dwell.vital_br_mean = vital_br
        dwell.insert(ignore_permissions=True)
        frappe.db.commit()
        return {"status": "entered", "dwell_name": dwell.name}
    elif event_type == "exit":
        # Close the most recent open dwell for this patient+zone
        open_dwell = frappe.get_value(
            "Patient Zone Dwell",
            {"patient_token": patient_token, "zone": zone_name_val, "time_out": ("is", "not set")},
            "name",
            order_by="time_in desc",
        )
        if open_dwell:
            doc = frappe.get_doc("Patient Zone Dwell", open_dwell)
            doc.time_out = ts
            if vital_hr:
                doc.vital_hr_mean = vital_hr
            if vital_br:
                doc.vital_br_mean = vital_br
            doc.save(ignore_permissions=True)
            frappe.db.commit()
            return {"status": "exited", "dwell_seconds": doc.dwell_seconds}
        return {"status": "no_open_dwell"}
    return {"status": "invalid_event_type"}


@frappe.whitelist(allow_guest=False)
def checkout_patient(patient_token: str) -> dict:
    """Mark patient visit as completed when they leave the clinic."""
    visit_name = frappe.get_value(
        "Patient Visit", {"patient_token": patient_token, "status": "Active"}, "name"
    )
    if not visit_name:
        return {"status": "no_active_visit"}
    frappe.db.set_value("Patient Visit", visit_name, {
        "status": "Completed",
        "check_out_time": frappe.utils.now_datetime(),
    })
    # Close any open dwells
    open_dwells = frappe.get_all(
        "Patient Zone Dwell",
        filters={"patient_token": patient_token, "time_out": ("is", "not set")},
        pluck="name",
    )
    now = frappe.utils.now_datetime()
    for d in open_dwells:
        doc = frappe.get_doc("Patient Zone Dwell", d)
        doc.time_out = now
        doc.save(ignore_permissions=True)
    frappe.db.commit()
    return {"status": "completed", "visit_name": visit_name}


@frappe.whitelist(allow_guest=False)
def get_zone_analytics(zone_id: str, days: int = 7) -> dict:
    """Return dwell time percentiles, throughput, and queue depth for a zone."""
    import statistics
    zone_name_val = frappe.get_value("Zone", {"zone_id": zone_id}, "name")
    if not zone_name_val:
        return {"error": "zone_not_found"}
    since = frappe.utils.add_days(frappe.utils.today(), -int(days))
    dwells = frappe.get_all(
        "Patient Zone Dwell",
        filters={"zone": zone_name_val, "time_out": ("is", "set"), "time_in": (">=", since)},
        fields=["dwell_seconds", "time_in"],
    )
    if not dwells:
        return {"zone_id": zone_id, "sample_count": 0}
    seconds = [d.dwell_seconds for d in dwells if d.dwell_seconds]
    sorted_s = sorted(seconds)
    n = len(sorted_s)

    def percentile(lst, p):
        idx = int(len(lst) * p / 100)
        return lst[min(idx, len(lst) - 1)]

    # Current occupancy (open dwells)
    current_count = frappe.db.count(
        "Patient Zone Dwell",
        filters={"zone": zone_name_val, "time_out": ("is", "not set")},
    )
    return {
        "zone_id": zone_id,
        "sample_count": n,
        "wait_p50_seconds": percentile(sorted_s, 50),
        "wait_p90_seconds": percentile(sorted_s, 90),
        "wait_mean_seconds": int(statistics.mean(seconds)) if seconds else 0,
        "throughput_per_day": round(n / max(int(days), 1), 1),
        "current_occupancy": current_count,
    }


@frappe.whitelist(allow_guest=False)
def get_patient_journey(visit_date: str = None) -> dict:
    """Return zone transition data for Sankey/flow visualization."""
    if not visit_date:
        visit_date = frappe.utils.today()
    visits = frappe.get_all(
        "Patient Visit",
        filters={"visit_date": visit_date},
        fields=["name", "patient_token", "patient"],
    )
    journeys = []
    for v in visits:
        dwells = frappe.get_all(
            "Patient Zone Dwell",
            filters={"patient_visit": v.name},
            fields=["zone_name", "time_in", "time_out", "dwell_seconds"],
            order_by="time_in asc",
        )
        journeys.append({
            "patient_token": v.patient_token,
            "zones": [{"zone": d.zone_name, "dwell_seconds": d.dwell_seconds} for d in dwells],
        })
    # Build Sankey links: count transitions A->B
    links: dict = {}
    for j in journeys:
        zones = j["zones"]
        for i in range(len(zones) - 1):
            key = (zones[i]["zone"], zones[i + 1]["zone"])
            links[key] = links.get(key, 0) + 1
    return {
        "date": visit_date,
        "visit_count": len(visits),
        "sankey_links": [{"from": k[0], "to": k[1], "value": v} for k, v in links.items()],
        "journeys": journeys,
    }


@frappe.whitelist(allow_guest=False)
def simulate_queue_capacity(zone_id: str, current_servers: int, arrival_rate_per_hour: float, mean_service_minutes: float) -> dict:
    """Erlang-C queuing simulation: current vs +1 server.
    Returns wait time estimates and utilization for 1, 2, and 3 servers."""
    import math

    def erlang_c(c: int, lam: float, mu: float) -> dict:
        """M/M/c Erlang-C formula. lam = arrivals/min, mu = service rate/min."""
        rho = lam / mu        # total offered load (Erlangs)
        a = rho / c           # utilization per server
        if a >= 1.0:
            return {"utilization": a, "wait_p50_seconds": 9999, "wait_p90_seconds": 9999, "feasible": False}
        # P0 computation
        sum_terms = sum((rho ** k) / math.factorial(k) for k in range(c))
        last_term = (rho ** c) / (math.factorial(c) * (1 - a))
        p0 = 1.0 / (sum_terms + last_term)
        # Probability of waiting (Erlang-C)
        pc = ((rho ** c) / (math.factorial(c) * (1 - a))) * p0
        # Mean wait in queue (minutes)
        wq = pc / (c * mu - lam)
        return {
            "utilization": round(a, 3),
            "mean_wait_seconds": round(wq * 60, 1),
            "wait_p50_seconds": round(wq * 60 * 0.693, 1),   # ~ln2 * mean for M/M/c
            "wait_p90_seconds": round(wq * 60 * 2.303, 1),   # ~ln10 * mean
            "feasible": True,
        }

    lam = arrival_rate_per_hour / 60.0  # arrivals per minute
    mu = 1.0 / mean_service_minutes      # service rate per server per minute
    results = {}
    for c in range(max(1, int(current_servers) - 1), int(current_servers) + 3):
        results[str(c)] = erlang_c(c, lam, mu)
    return {
        "zone_id": zone_id,
        "current_servers": current_servers,
        "arrival_rate_per_hour": arrival_rate_per_hour,
        "mean_service_minutes": mean_service_minutes,
        "scenarios": results,
        "recommendation": next(
            (str(c) for c in range(1, int(current_servers) + 3)
             if results.get(str(c), {}).get("wait_p90_seconds", 9999) < 300
             and results.get(str(c), {}).get("feasible", False)),
            str(current_servers),
        ),
    }


@frappe.whitelist(allow_guest=False)
def get_active_patients() -> dict:
    """Return all patients currently in the clinic with their current zone."""
    active = frappe.get_all(
        "Patient Visit",
        filters={"status": "Active"},
        fields=["name", "patient", "patient_token", "check_in_time"],
    )
    result = []
    for v in active:
        current_zone = frappe.get_value(
            "Patient Zone Dwell",
            {"patient_token": v.patient_token, "time_out": ("is", "not set")},
            ["zone_name", "time_in"],
            as_dict=True,
            order_by="time_in desc",
        )
        result.append({
            "patient": v.patient,
            "patient_token": v.patient_token,
            "check_in_time": str(v.check_in_time),
            "current_zone": current_zone.zone_name if current_zone else "Unknown",
            "zone_since": str(current_zone.time_in) if current_zone else None,
        })
    return {"active_count": len(result), "patients": result}
