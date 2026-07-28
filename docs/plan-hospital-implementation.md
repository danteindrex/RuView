# Implementation Plan — Hospital-Grade Medical & Security (tabs, endpoints, integrations)

Implementation detail for `docs/plan-hospital-grade-roadmap.md`. Answers: **exactly
how it works**, **which tabs**, **which new endpoints**, **which integrations**, and
**what Frappe already gives us for hardware**.

---

## 0. What Frappe/ERPNext already gives us (checked)

**We do not build patient identity or a device bridge from scratch — Frappe has both.**

| Capability | What exists | How we use it |
|-----------|-------------|---------------|
| **Clinical data model** | ERPNext **Healthcare** module/app: `Patient`, `Patient Encounter`, **`Vital Signs`** DocTypes. Our `ruview_care` **already `required_apps = ["erpnext"]`** (`hooks.py:10`). | Install `healthcare`; link `CSI Session`→`Patient`; write our vitals into ERPNext **`Vital Signs`** — instant patient-centric clinical records (the audit's #3 gap). |
| **Hardware sync pattern** | Official **`biometric-attendance-sync-tool`** (ZKTeco / ZKProtocol TCP/IP), plus **HTTP/MQTT real-time push** → `Employee Checkin`. Documented **IoT via MQTT broker → ERPNext**. | Mirror this as a **RuView Device Bridge**: MQTT/HL7 ingest → DocTypes. Lets Frappe pull in *other* hardware (3rd-party vitals monitors, bed sensors, RTLS badges, door controllers). |
| **Background jobs / scheduler** | RQ workers + `scheduler_events` (already used by the insight pipeline). | Device polling, ADT sync, EHR push retries, alarm escalation timers. |
| **RBAC + multi-site** | ruview_care roles + `RuView Deployment` fleet. | Reuse for ward/site scoping of patients, alarms, zones. |

**Answer to "can Frappe connect to more hardware?": yes** — reuse the Healthcare
DocTypes for the clinical side, and clone the biometric/MQTT sync pattern into a
generic device bridge for everything else.

---

## 1. How it works (end-to-end)

```
                                   ┌─────────── EHR (Epic / Oracle Health / Meditech) ───────────┐
                                   │  FHIR Observation/Device/Patient   ·   HL7 v2 PCD-01/ADT    │
                                   └───────▲───────────────────────────────────────▲────────────┘
 WiFi/CSI nodes ─UDP→ sensing-server ──────┼──────────► ruview_care (Frappe) ───────┘
   (vitals, presence, falls,               │  REST      + ERPNext Healthcare
    on-device inference 0xC5110009)   FHIR emitter        (Patient, Vital Signs, Encounter)
        │                                   │                    │
        │ WS /ws/sensing                    │                    ├─► Nurse-call (Ascom/Rauland)
        ▼                                   │                    ├─► Clinical Alarm mgr (IEC 60601-1-8)
   Desktop app (Medical / Security hubs)◄───┘                    ├─► VMS (ONVIF) / Access (OSDP)
        ▲                                                        └─► Central station (SIA DC-09)
        │  inbound: RuView Device Bridge (MQTT / HL7 listener) ◄── 3rd-party vitals/bed/RTLS/door hardware
```

- **Sensing stays as-is** (DSP + on-device neural). New work is the **data model
  (patient/bed), the export/alarm layers, and the integration adapters** —
  the "system" around the sensor the audit said was missing.
- Every clinical feature ships **non-diagnostic** until Phase C clearance; the
  medical monitor claim is sequenced last (see roadmap §3 Phase C).

---

## 2. Exact UI tabs

### Medical Hub → restructure `medical-page.tsx` into tabs
| Tab | Purpose | Data source |
|-----|---------|-------------|
| **Live** | Per-**patient/bed** vitals + presence + posture (today's cards, but bound to a patient) | `/ws/sensing`, `vital_signs` |
| **Rounding** | Oxevision-style **verified safety check** workflow (present? moving? breathing?) + audit trail; explicitly non-diagnostic | new `/api/v1/rounding/*` |
| **Patient** | Bind node/bed → ERPNext `Patient`; show encounter link, admission info (ADT) | Frappe Healthcare |
| **Alarms** | **IEC 60601-1-8** priority list (high/med/low), latch, **acknowledge**, escalate, silence-with-reason | new `/api/v1/alarms/*` |
| **AI Insights** | existing LangGraph panel — relabelled **"AI Summary (non-diagnostic)"**, drop "differential/urgency" as clinical | existing |
| **Analytics** | HR/BR trends + risk distribution (Cloud tier) | existing |
| **Integrations** | FHIR/EHR/nurse-call connection + last-sync status | new config commands |

### Security Hub → restructure `security-page.tsx` into tabs
| Tab | Purpose | Data source |
|-----|---------|-------------|
| **Live** | Occupancy, presence, per-zone status | `/ws/sensing` |
| **Zones** | Define named zones, per-zone rules, **verified-human filter** (pet/mechanical reject — Origin TruPresence parity) | new `/api/v1/zones/*` |
| **Arming** | Partitions, arming **schedules**, entry/exit delays, bypass, user codes | new `/api/v1/arming/*` |
| **Events** | **Tamper-evident** security event log (arm/disarm/alarm/ack) | new `/api/v1/events/security` |
| **Integrations** | VMS / access-control / central-station status | new config commands |
> Move **SSH Key Management** out of Security → System Admin (it's devops, not security).

### New top-level pages
| Page | Purpose |
|------|---------|
| **Patients** | Ward roster + **bed map**, patient↔node binding, admission status (only when the clinical path is enabled) |
| **Devices** | The **RuView Device Bridge**: 3rd-party hardware connected via MQTT/HL7/biometric-style sync, health + last packet |

---

## 3. New endpoints

### Sensing-server REST (new routes in `main.rs`)
| Method + path | Purpose |
|---------------|---------|
| `POST /api/v1/patients/{pid}/bind` · `DELETE …/bind` | bind/unbind a node or bed to a patient |
| `GET /api/v1/patients` · `GET /api/v1/beds` | roster + bed map |
| `GET/POST /api/v1/rounding` · `POST /api/v1/rounding/{id}/ack` | rounding checks + sign-off |
| `GET /api/v1/alarms` · `POST /api/v1/alarms/{id}/ack` · `/escalate` · `/silence` | clinical alarm lifecycle |
| `GET/POST /api/v1/zones` · `PUT /api/v1/zones/{id}` | security zones + rules |
| `GET/POST /api/v1/arming` | partitions + schedules |
| `GET /api/v1/events/security` | security event log |
| `GET /api/v1/export/fhir/{session}` | FHIR `Bundle` (Observation/Device/Patient) for the session |

### Tauri commands (new, desktop)
- **Patient/bed:** `list_patients`, `list_beds`, `bind_node_patient`, `set_adt_config`.
- **Rounding:** `record_round`, `list_rounds`.
- **Alarms:** `list_alarms`, `ack_alarm`, `escalate_alarm`, `silence_alarm`.
- **Zones/arming:** `list_zones`, `set_zone`, `list_arming`, `set_arming_schedule`.
- **Integrations config (keychain-backed):** `set_fhir_endpoint`, `set_nursecall_config`, `set_vms_config`, `set_access_control_config`, `set_central_station_config`.
- **Device bridge:** `list_bridge_devices`, `set_device_bridge`, `bridge_status`.

### Frappe API (new methods in `ruview_care/api.py`)
- `bind_patient(deployment_id, node_id, patient)` → link.
- `ingest_vitals(patient, hr, br, ts, source, confidence)` → write ERPNext **`Vital Signs`** doc.
- `create_rounding_record(patient, present, moving, breathing, staff, ts)`.
- `ingest_device(device_id, kind, payload)` → generic **Device Bridge** ingest (MQTT/HL7 → DocType).
- `raise_clinical_alarm(patient, priority, kind)` / `ack_alarm(name, user)`.

### New adapter services (small, standalone)
| Adapter | Standard/protocol | Endpoint it calls / listens on |
|---------|-------------------|--------------------------------|
| **FHIR emitter** | HL7 FHIR R4 "Devices on FHIR" | `POST {ehr_fhir}/Observation` (HR LOINC `8867-4`, RR `9279-1`), `Device`, `Patient` |
| **HL7 v2 out** | IHE **PCD-01** `ORU^R01` | via **Mirth Connect** to the EHR interface engine |
| **ADT in** | HL7 v2 **`ADT^A01/A02/A03`** | MLLP listener → patient/bed model |
| **Nurse-call** | Ascom **Telligence/Digistat** / Rauland | partner API / event push |
| **VMS** | **ONVIF** Profile A/C + Genetec/Milestone SDK | analytics-event push onto the camera timeline |
| **Access control** | **OSDP** (SIA) | presence/tailgating events to panels |
| **Central station** | **SIA DC-09** (UL 827/1981) | alarm to a monitoring receiver + relay/siren |
| **Device Bridge** | **MQTT** + HL7 MLLP (biometric-sync pattern) | subscribe/listen → `ingest_device` |

### New Frappe DocTypes (ruview_care) — reuse Healthcare where possible
- **Reuse (ERPNext Healthcare):** `Patient`, `Patient Encounter`, **`Vital Signs`**.
- **New:** `Bed` (room/bed ↔ deployment/node), `Rounding Record`, `Clinical Alarm`, `Security Zone`, `Arming Schedule`, `Security Event`, `Connected Device` (bridge inventory).

---

## 4. Phasing (maps to the roadmap)

- **Phase A (weeks, no clearance):** install `healthcare`; `Bed`/binding + `Patient` link; `ingest_vitals`→`Vital Signs`; **FHIR emitter** (validate on HAPI FHIR); **Rounding** tab + DocType; Security **Zones/Arming/Events** tabs + verified-human filter; move SSH keys out; **Device Bridge** MVP (MQTT ingest).
- **Phase B (months):** **Alarms** tab + `Clinical Alarm` + IEC 60601-1-8 model; **nurse-call**, **VMS/ONVIF**, **OSDP**, **SIA DC-09** adapters; **ADT** consumer; PCD-01 via Mirth; reliability/failover.
- **Phase C (12–24 mo, $$):** clinical validation + **FDA De Novo/510(k)** or **CE MDR**; until then everything medical stays **non-diagnostic**.

---

## Sources
- [ERPNext Healthcare — Patient Encounter & Vital Signs (docs)](https://docs.erpnext.com/docs/v13/user/manual/en/healthcare/patient_encounter)
- [Complete Technical Guide to ERPNext Healthcare](https://nexeves.com/blog/ERPNext/complete-technical-guide-to-healthcare-module-in-erpnext)
- [Frappe HR — integrating biometric attendance devices](https://docs.frappe.io/hr/integrating-frappe-hr-with-biometric-attendance-devices)
- [frappe/biometric-attendance-sync-tool (GitHub)](https://github.com/frappe/biometric-attendance-sync-tool)
- [IoT Integration with ERPNext (MQTT)](https://clefincode.com/blog/global-digital-vibes/en/iot-integration-with-erpnext-bringing-the-physical-world-into-your-erp)
- [Devices on FHIR / vital signs representation (ONC ISP)](https://isp.healthit.gov/representing-patient-vital-signs)
