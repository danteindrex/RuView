app_name = "ruview_care"
app_title = "RuView Care"
app_publisher = "RuView"
app_description = "WiFi DensePose deployment management — multi-site hospital monitoring"
app_email = "admin@ruview.io"
app_license = "Proprietary"
app_version = "0.1.0"

# Required apps. `healthcare` provides the Patient / Patient Encounter /
# Vital Signs DocTypes we reuse for the clinical record (we do not reinvent
# patient identity or vitals storage).
required_apps = ["erpnext", "healthcare"]

# Roles created on install
roles = [
    {"role_name": "RuView Enterprise Admin",  "desk_access": 1},
    {"role_name": "RuView Clinical Staff",    "desk_access": 1},
    {"role_name": "RuView Operator",          "desk_access": 1},
    {"role_name": "RuView Viewer",            "desk_access": 1},
]

# Scheduled tasks
scheduler_events = {
    "all": [
        "ruview_care.ruview_care.tasks.update_deployment_heartbeats",
    ],
    "hourly": [
        "ruview_care.ruview_care.tasks.generate_risk_alerts",
    ],
}

# Webhook endpoints exposed via Frappe's REST API
# External systems POST to /api/method/ruview_care.api.*
