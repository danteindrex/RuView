import frappe
from frappe.model.document import Document
from frappe.utils import time_diff_in_seconds


class PatientZoneDwell(Document):
    def on_update(self):
        if self.time_out and self.time_in:
            self.dwell_seconds = int(
                time_diff_in_seconds(self.time_out, self.time_in)
            )
            self.db_update()
