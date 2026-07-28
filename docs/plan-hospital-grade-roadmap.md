# Plan — From Sensing Layer to Hospital-Grade Medical & Security

Follow-up to `docs/audit-medical-security-hubs.md` (which found both hubs are ~1–2
features, not systems). This plan benchmarks the **best commercial systems**,
extracts the features/attributes that make them *adoptable*, lists **concrete
integration targets** (with the actual standards/protocols), and sequences the
work against our codebase — honest about which items are quick wins vs the
multi-year regulatory rock.

---

## 0. Benchmark — who's best, and why they're adoptable

### Medical (contactless / ambient monitoring)
| System | Tech | Why hospitals adopt it |
|--------|------|------------------------|
| **Oxehealth Oxevision** | IR camera | **The model to copy.** FDA De Novo (vital signs) + 510(k) (sleep) + CE; **clinically validated vs polysomnography**; a **"digital rounding"** workflow; deployed across ~half of NHS England mental-health trusts; responded to 1,453 safety incidents in one trust over 4 yrs. |
| **Xandar Kardian XK300** | 60 GHz UWB radar | First **FDA 510(k) Class II** contactless HR/RR/motion/presence monitor; general hospitals + SNF/AL. Reference for radar-grade accuracy. |
| **Vayyar Care** | mmWave radar | Cameraless **fall detection** + bed-exit; privacy-preserving; senior-living scale. |
| **Neteera, Sleepiz, Oxehealth** | radar/camera | Strong on **clinical validation + IP + health-system partnerships** — the moat is validation, not the sensor. |

**Lesson:** the differentiator is **validation + regulatory + EHR/nurse-call integration + a clinical workflow**, not the sensing modality. Our RF layer is competitive; the *system* around it is the gap.

### Security (our closest competitor is literally WiFi-sensing)
| System | Tech | Adoptable features |
|--------|------|--------------------|
| **Origin AI (Hex Home / TruShield / TruPresence)** | **WiFi sensing (our exact tech)** | **Zone-level intrusion**, **human-vs-pet/mechanical filtering** ("verified presence"), self-learning sensitivity, see-through-wall, blind-spot-free, "AI Sensing" analytics platform. This is the productization blueprint for our sensor. |
| **Genetec / Milestone (VMS)** | video mgmt | Event timeline, analytics ingestion, unified ops — the **system of record** RF events should feed into (ONVIF). |
| **LenelS2 / Software House C•CURE (access control)** | badge/door | **OSDP** device bus; presence/tailgating events. |
| **UL-listed alarm + central station** | monitored dispatch | **Monitored** alarm path (UL 827/1981), **SIA DC-09** to a receiver, relay/siren outputs, escalation. |

**Lesson:** Origin AI shows the *feature* set to build; Genetec/OSDP/central-station show the *integrations* that turn a sensor into a security system.

---

## 1. Features to adopt (mapped to our code)

### Medical
1. **Validated vitals with confidence** — replace the unvalidated (and, on Pi, *synthetic* — `edge_dsp.rs:94`) HR/BR with real DSP + a **confidence/quality** field and a **reference-device comparison harness** (target Oxevision/XK accuracy). *Data source already exists (`vital_signs`, `edge_vitals`).*
2. **Digital rounding / verified safety check** — Oxevision's lowest-regulatory, highest-value feature: a periodic "is the patient present/moving/breathing" check with an **audit trail**, explicitly **non-diagnostic**. Build on the existing Medical Hub + Frappe DocTypes.
3. **Sleep/wake + bed occupancy** — from presence + motion + coherence we already compute.
4. **Fall + bed-exit / get-up prediction** — extend the existing fall flag + `intention.rs` (pre-movement lead signals) toward Vayyar-style bed-exit.
5. **Patient-centric sessions** — bind sessions to a **Patient + Bed**, not `session-${Date.now()}`.

### Security
1. **Zone-level intrusion + human/pet/mechanical filtering** — we already have the **pose tracker** + **adversarial gating** (`ruvsense/adversarial.rs`) + coherence; productize named **zones** and a "verified human presence" gate (TruPresence parity).
2. **Arming schedules, partitions, entry/exit delays, bypass, user codes** — replace the single `security_armed` boolean.
3. **Self-learning baseline sensitivity** per room (ties into the per-room LoRA work).
4. **Tamper-evident security event log** (arm/disarm/alarm/ack) distinct from the admin audit.

---

## 2. Integration targets (concrete — standard/protocol → product)

### Medical device / EHR ecosystem
| Target | Standard / protocol | How |
|--------|--------------------|-----|
| **EHR vitals write-back (Epic, Oracle Health/Cerner, Meditech)** | **HL7 FHIR R4 "Devices on FHIR"** — `Observation` (LOINC-coded HR 8867-4, RR 9279-1), `Device`, `Patient`; fall as `Observation`/`Flag` | A small FHIR emitter service (Rust/Python) → hospital FHIR endpoint; validate against **HAPI FHIR** test server first. Epic via USCDI/FHIR, Oracle Health via their FHIR APIs. |
| **Device→EHR where FHIR isn't available** | **IHE PCD-01** (HL7 v2 `ORU^R01`) | Through an integration engine (**Mirth Connect**) or **Ascom Digistat** MDI. |
| **Patient identity + bed mapping** | **HL7 v2 ADT** (`A01/A02/A03`) | Consume the hospital ADT feed → our Patient/Bed model. |
| **Nurse-call** | **Ascom Telligence / Healthcare Platform (Digistat)**, **Rauland Responder** | Ascom **interoperability-partner** program; push our alerts as nurse-call events with priority. |
| **Clinical alarm management** | **IEC 60601-1-8** | Model alarm priority/latching/escalation/silence-with-reason before any monitored claim. |

### Security ecosystem
| Target | Standard / protocol | How |
|--------|--------------------|-----|
| **VMS (Genetec, Milestone)** | **ONVIF Profile A/C** + vendor SDK | Push RF presence/intrusion/zone events onto the VMS timeline (correlate with cameras). |
| **Access control (LenelS2, C•CURE)** | **OSDP** (SIA) | Presence/tailgating events to panels. |
| **Monitored alarm / central station** | **SIA DC-09**, UL 827/1981 | Bridge alarms to a monitoring receiver; add **relay/siren** outputs + duress. |
| **SIEM / ops** | syslog/CEF, webhook | Ship security events to enterprise SIEM. |

---

## 3. Phased roadmap (tied to the codebase)

### Phase A — Adjunct-grade, low regulatory (quick wins, weeks)
- **Patient/Bed data model** — extend the Frappe app (`ruview_care`): `Patient`, `Bed`, link `CSI Session` → Patient/Bed; ADT ingest stub.
- **FHIR emitter** — new service emits `Observation`/`Device`/`Patient`; test against HAPI FHIR. *(Makes vitals/falls consumable by any EHR.)*
- **Digital rounding tool** — Medical Hub: periodic verified check + audit; labelled **non-diagnostic**.
- **Security zones + schedules + verified-presence filter** + **tamper-evident event log**. Move SSH-key mgmt out of the Security page.
- **Vitals confidence + reference-comparison harness** (accuracy telemetry).

### Phase B — Integrations (make it a *system*, months)
- **Nurse-call adapter** (Ascom/Rauland) via an alert bridge; **IEC 60601-1-8** alarm model (priority/escalation/ack).
- **VMS + access-control adapters** (ONVIF event push, OSDP); **central-station SIA DC-09** bridge + relay outputs.
- **IHE PCD-01** path via Mirth for non-FHIR EHRs; **ADT** consumer for real patient binding.
- Reliability: server failover, monitoring-gap SLA, data-loss detection.

### Phase C — Regulatory (the big rock — only for the *clinical monitor* claim)
- **Clinical validation study** (accuracy vs reference monitors; fall sensitivity/specificity) → **FDA De Novo/510(k)** or **EU MDR CE**; **ISO 13485** QMS. 12–24+ months, significant $$.
- Until cleared: ship medical features as **wellness / ambient-awareness / rounding**, explicitly **non-diagnostic** (Oxevision shipped rounding + safety before the monitor claim). Do **not** present LLM "differential/urgency" as clinical guidance.

---

## 4. Honest sequencing

- **Phases A + B make us a credible *adjunct*** — camera-free presence/occupancy/rounding + falls that *feed* the hospital's existing monitors, nurse-call, EHR, VMS, and access control. That is sellable **without** FDA clearance if positioned as non-diagnostic ambient awareness + security analytics.
- **Phase C is what a true "patient monitor" or "life-safety security system" claim requires** — sequence it deliberately; **do not gate the adjunct features on it.**
- Our genuine edge vs Origin AI (security) and vs radar/camera incumbents (medical): **the same cheap WiFi mesh does both**, plus an **ERP/fleet backend** most point-solutions lack. Lead with **integration + multi-site management**, not with unvalidated clinical claims.

---

## Sources
- [FDA clears radar-powered, contactless patient monitor from Xandar Kardian — Fierce Biotech](https://www.fiercebiotech.com/medtech/fda-clears-radar-powered-contactless-patient-monitor-from-xandar-kardian)
- [Xandar Kardian — Contactless, Continuous Patient Monitoring](https://xkcorp.com/)
- [FDA grants Oxehealth Vital Signs De Novo clearance](https://www.prnewswire.com/news-releases/fda-grants-oxehealth-vital-signs-de-novo-clearance-301259496.html)
- [Oxehealth secures FDA and CE mark for sleep monitoring](https://www.nsmedicaldevices.com/news/oxehealth-sleep-monitoring-solution/)
- [Oxehealth — supporting safer patient care (Oxford)](https://eng.ox.ac.uk/case-studies/oxehealth-supporting-safer-patient-care)
- [Comparison of ISO/IEEE 11073, IHE PCD-01, and HL7 FHIR for personal health devices](https://e-hir.org/journal/view.php?id=10.4258%2Fhir.2018.24.1.46)
- [Representing Patient Vital Signs — ONC Interoperability Standards Platform](https://isp.healthit.gov/representing-patient-vital-signs)
- [Ascom Telligence nurse call + interoperability partners](https://www.ascom.com/about-us/why-ascom/interoperability-partners/)
- [Origin Wireless — WiFi Sensing / TruPresence](https://www.originwirelessai.com/trupresence-2/)
- [Origin AI — Zone Detection & AI Sensing platform](https://www.prnewswire.com/news-releases/origin-ai-introduces-smarter-home-security-with-new-zone-detection-and-ai-sensing-platform-302538109.html)
- [Hex Home — WiFi-sensing home security (TechHive)](https://www.techhive.com/article/579129/hex-home-security-monitors-your-wi-fi-network-to-detect-intruders.html)
