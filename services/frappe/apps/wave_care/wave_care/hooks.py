app_name = "wave_care"
app_title = "Wave Care"
app_publisher = "Wave"
app_description = "WiFi DensePose deployment management — multi-site hospital monitoring"
app_email = "admin@wave.io"
app_license = "Proprietary"
app_version = "0.1.0"

# Required apps. `healthcare` provides the Patient / Patient Encounter /
# Vital Signs DocTypes we reuse for the clinical record (we do not reinvent
# patient identity or vitals storage).
required_apps = ["erpnext", "healthcare"]

# Roles created on install
roles = [
    {"role_name": "Wave Enterprise Admin",  "desk_access": 1},
    {"role_name": "Wave Clinical Staff",    "desk_access": 1},
    {"role_name": "Wave Operator",          "desk_access": 1},
    {"role_name": "Wave Viewer",            "desk_access": 1},
]

# Scheduled tasks
scheduler_events = {
    "all": [
        "wave_care.wave_care.tasks.update_deployment_heartbeats",
    ],
    "hourly": [
        "wave_care.wave_care.tasks.generate_risk_alerts",
    ],
}

# Webhook endpoints exposed via Frappe's REST API
# External systems POST to /api/method/wave_care.api.*
