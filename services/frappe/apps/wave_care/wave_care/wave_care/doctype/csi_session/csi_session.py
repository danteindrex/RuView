import frappe
from frappe.model.document import Document

class CsiSession(Document):
    def before_insert(self):
        if not self.session_time:
            self.session_time = frappe.utils.now_datetime()
