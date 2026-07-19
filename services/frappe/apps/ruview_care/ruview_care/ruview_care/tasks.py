import frappe
from datetime import datetime, timezone, timedelta

def update_deployment_heartbeats():
    """Mark deployments offline if no heartbeat in 5 minutes."""
    cutoff = frappe.utils.add_to_date(frappe.utils.now_datetime(), minutes=-5)
    stale = frappe.get_all("RuView Deployment",
                           filters={"status": "Online", "last_seen": ["<", cutoff]},
                           fields=["name"])
    for d in stale:
        frappe.db.set_value("RuView Deployment", d.name, "status", "Offline")
    if stale:
        frappe.db.commit()

def generate_risk_alerts():
    """Hourly: create alerts for any deployment with high/critical risk that has no open alert."""
    high_risk = frappe.get_all("RuView Deployment",
                               filters={"active_risk_level": ["in", ["high", "critical"]], "status": "Online"},
                               fields=["name", "deployment_name", "location_name", "active_risk_level"])
    for d in high_risk:
        existing = frappe.db.exists("Risk Alert", {"deployment": d.name, "status": "Open"})
        if not existing:
            frappe.get_doc({
                "doctype": "Risk Alert",
                "deployment": d.name,
                "deployment_name": d.deployment_name,
                "location": d.location_name,
                "risk_level": d.active_risk_level,
                "status": "Open",
                "alert_time": frappe.utils.now_datetime(),
            }).insert(ignore_permissions=True)
    frappe.db.commit()
