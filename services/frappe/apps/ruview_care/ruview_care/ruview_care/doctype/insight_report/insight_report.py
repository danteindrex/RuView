import frappe
from frappe.model.document import Document

class InsightReport(Document):
    def after_insert(self):
        if self.risk_level in ("high", "critical") and self.deployment:
            dep = frappe.get_doc("RuView Deployment", self.deployment)
            dep.update_heartbeat(risk_level=self.risk_level)
