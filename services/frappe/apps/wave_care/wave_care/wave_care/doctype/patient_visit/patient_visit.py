import frappe
import uuid
from frappe.model.document import Document
from frappe.utils import now_datetime, time_diff_in_seconds


class PatientVisit(Document):
    def before_insert(self):
        if not self.patient_token:
            self.patient_token = str(uuid.uuid4())
        if not self.visit_date:
            self.visit_date = frappe.utils.today()
        if not self.check_in_time:
            self.check_in_time = now_datetime()

    def on_update(self):
        if self.check_out_time and self.check_in_time:
            self.total_duration_seconds = int(
                time_diff_in_seconds(self.check_out_time, self.check_in_time)
            )
            self.db_update()
