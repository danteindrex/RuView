import frappe
from frappe.model.document import Document
from datetime import datetime, timezone

class RuViewDeployment(Document):
    def before_save(self):
        if not self.registered_at:
            self.registered_at = frappe.utils.now_datetime()

    def update_heartbeat(self, node_count=None, risk_level=None):
        self.last_seen = frappe.utils.now_datetime()
        self.status = "Online"
        if node_count is not None:
            self.node_count = node_count
        if risk_level:
            self.active_risk_level = risk_level
            if risk_level in ("high", "critical"):
                self._maybe_create_risk_alert(risk_level)
        self.save(ignore_permissions=True)

    def _maybe_create_risk_alert(self, risk_level):
        existing = frappe.db.exists("Risk Alert", {
            "deployment": self.name,
            "status": "Open",
            "risk_level": risk_level,
        })
        if not existing:
            frappe.get_doc({
                "doctype": "Risk Alert",
                "deployment": self.name,
                "deployment_name": self.deployment_name,
                "location": self.location_name,
                "risk_level": risk_level,
                "status": "Open",
                "alert_time": frappe.utils.now_datetime(),
            }).insert(ignore_permissions=True)
