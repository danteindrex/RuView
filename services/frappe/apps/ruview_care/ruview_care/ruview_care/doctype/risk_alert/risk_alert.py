import frappe
from frappe.model.document import Document

class RiskAlert(Document):
    def on_update(self):
        if self.status == "Acknowledged" and not self.acknowledged_by:
            self.acknowledged_by = frappe.session.user
