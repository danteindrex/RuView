# Audit — Medical Hub & Security Center: can a hospital actually use them?

**Verdict up front:** No. Both are **1–2 real features wrapped in an ambitious UI**, not
hospital-grade *systems*. The Medical Hub is a **contactless vitals estimate + a
fall/presence flag + an LLM "insight" panel** — a research/wellness dashboard, not a
clinical monitoring system. The Security Center is a **privacy-preserving RF
occupancy/presence sensor + a WhatsApp notification** — not a security system a
hospital could run intrusion/access operations on. Neither has the integration,
validation, regulatory, or reliability layer that makes a hospital system adoptable.

This audit is grounded in the actual page code and a whole-repo integration scan.

---

## A. Medical Hub (`ui-v2/src/pages/medical-page.tsx`)

### What it actually does (from the code)
- **Live vitals cards** — heart rate + respiration from `latestUpdate.vital_signs` (CSI DSP zero-crossing), with sparklines; posture/activity; an "Edge Node Vitals" card from the node packet.
- **Fall flag** — `posture === "lying_down"` OR `edgeVitals.fall_detected` → a pulsing "EMERGENCY: FALL DETECTED" badge (medical-page.tsx:115, 228).
- **"AI Insights" tab** — a button POSTs the *current* vitals to the Frappe **LangGraph GPT-4o-mini** pipeline (`run_insight_pipeline`), polls for an Insight Report, and renders a **risk gauge, "clinical interpretation" (primary findings, differential, recommended actions, urgency: routine/urgent/emergency), fall/CV/respiratory sub-scores** (medical-page.tsx:119-215, 20-50).
- **Analytics tab** — HR/BR trend + risk-distribution charts (Cloud tier only).

### Why a hospital can't use it as a clinical system

1. **Not a regulated medical device.** Contactless vital-sign monitoring + fall detection + surfacing a **"differential"** and **"urgency: emergency"** is medical-device / clinical-decision-support software. That requires **FDA clearance (510(k)/De Novo)** or **EU MDR CE marking**, plus a quality system (ISO 13485). There is **zero** regulatory artifact, and the UI literally presents GPT-generated clinical text as guidance (medical-page.tsx:32-37, 169-178).

2. **The vitals are unvalidated — and partly synthetic.** HR/BR come from CSI DSP with **no accuracy validation** against reference monitors and no clinical error bounds. Worse, the **Pi edge DSP fabricates breathing/heart rate from a frame counter** (`edge_dsp.rs:94-95` — `breathing = 12 + (sequence % 30)*0.35`), i.e. a demo placeholder, not a measurement. A clinician cannot act on a number with no validated accuracy.

3. **There is no patient.** Sessions are `session-${Date.now()}` (medical-page.tsx:122). No **patient identity, MRN, admission/ADT feed, or bed mapping** — you cannot say *whose* vitals these are. Hospital monitoring is patient-centric; this is room-centric with no person binding.

4. **No EHR/EMR integration.** No **HL7 v2** or **FHIR** export, no flowsheet write-back, no orders/results. *(Whole-repo scan: no HL7/FHIR/EHR/EMR/ADT anywhere in the live code — only old worktree copies + a `v1/` Python test mock.)* Data dead-ends in local SQLite / a Frappe DocType.

5. **No clinical alarm management.** Real monitors implement **IEC 60601-1-8** (alarm priorities, latching, audio, silence-with-reason, escalation) and integrate with **nurse-call**. Here an "alarm" is a red badge + optional **WhatsApp message** — an unmonitored consumer channel, no acknowledgement audit, no escalation, no redundancy.

6. **No reliability/continuity.** Single hub process, best-effort UDP, no failover, no monitoring-gap SLA, no data-loss handling — disqualifying for continuous patient monitoring.

7. **The "AI clinical" layer is LLM narrative.** `insight_pipeline.py` is `gpt-4o-mini` with deterministic risk arithmetic wrapped in model text; **not trained or validated on clinical outcomes.** `differential` and `action_items` are hallucination-prone.

**Medical verdict:** genuinely interesting as an **ambient contactless-sensing signal** (presence, motion, a vitals *estimate*, a fall cue) — but it is a **demo/wellness dashboard, not a clinical monitoring system.** ~2 features, not a system.

---

## B. Security Center (`ui-v2/src/pages/security-page.tsx`)

### What it actually does (from the code)
- **Presence / occupancy** — RF presence (`classification.presence`), person count vs `crowd_threshold`, "Zone Integrity" (Active Motion / Zone Secure) (security-page.tsx:49-54, 183-228).
- **Arm/disarm** — a single global boolean in tenant settings (`security_armed`) (security-page.tsx:42-47).
- **Intrusion banner + "Intruder Details"** — zone / person-id / confidence / keypoints / bbox from the **pose tracker** (security-page.tsx:80-127).
- **Alert dispatch** — WhatsApp via WHAPI (`use-alert-system.ts`).
- **SSH Key Management card** — paste an SSH private key/passphrase (security-page.tsx:129-181). *(This is devops secret storage, not a security-domain feature.)*

### Why a hospital can't use it as a security system

1. **No integration with physical-security infrastructure.** No **access control (badge/door)**, **VMS/CCTV**, **SIEM**, **PSIM**, **alarm panel / central station**, or relay/siren outputs. A hospital security system is a coordinated ecosystem; this is a standalone RF presence sensor.

2. **Single, unmonitored alert channel.** Intrusion → a WhatsApp message. No monitored central station, guard-dispatch workflow, acknowledgement/escalation, siren/relay, or duress. WhatsApp is not a security-grade notification path.

3. **No zones / schedules / partitions.** "Armed" is one global flag — no per-area partitions, arming schedules, entry/exit delays, bypass, or user codes.

4. **No security event audit.** The generic auth audit logs admin CRUD, but there's no tamper-evident **security event log** (arm/disarm history, alarm timeline, response times) that security operations require.

5. **"Intruder details" is the synthetic pose tracker.** Zone/person-id/keypoints/bbox come from the *derived* (synthetic) pose, not a validated localized track; the confidence figures are cosmetic.

6. **Mixed concerns.** Bolting SSH key management into "Security Center" signals these are feature cards, not a coherent security product.

**Security verdict:** useful as a **privacy-preserving occupancy/motion sensor** (fall-risk room presence, crowd counting, "is anyone in this restricted room") — but **not** a security system for hospital access/intrusion/monitoring operations. Again ~1–2 features, not a system.

---

## C. "Even as a system, if they wanted to"

- The **ERP/Frappe layer** (deployments, RBAC, risk alerts, RQ pipeline, dashboards) is real, reusable infrastructure — it could underpin a **fleet-management / analytics** product across sites.
- But for **clinical or security operations**, the platform is missing the entire **integration + regulatory + reliability** ecosystem. What exists today is a strong **ambient RF-sensing layer + dashboards + a cloud management backend** — a promising **component / pilot platform**, not a turnkey hospital system.

Positioned honestly, the sellable value today is: **contactless, camera-free room monitoring** (elopement/fall-risk presence, occupancy, gross vitals trends) as an *adjunct* signal feeding a hospital's existing systems — **not** a replacement for patient monitors, nurse-call, or security/access platforms.

---

## D. What it would take to be genuinely hospital-usable

**Medical (to become a clinical system):**
1. Regulatory pathway — FDA 510(k)/De Novo or EU MDR CE; ISO 13485 QMS; a **clinical validation study** (accuracy vs reference monitors, sensitivity/specificity for falls).
2. **Patient identity** — ADT/HL7 feed, MRN, bed/room binding; sessions tied to a patient.
3. **FHIR** vitals `Observation` + fall `Observation`/`Flag` write-back to the EHR flowsheet.
4. **IEC 60601-1-8** alarm management + **nurse-call** integration; acknowledgement + escalation + audit.
5. Redundancy/failover, monitored uptime SLA, gap/data-loss detection.
6. Reposition or remove the LLM "differential/urgency" — it cannot be diagnostic without validation.

**Security (to become a security system):**
1. Integrate **access control / VMS / SIEM / alarm panel**; hardware relay/siren + duress outputs.
2. **Monitored central-station** dispatch with acknowledgement + escalation.
3. Zones/partitions, arming schedules, entry/exit delays, bypass, user codes.
4. Tamper-evident **security event log** (arm/disarm/alarm/response).
5. Move SSH key management out of the security product surface.

---

## E. Bottom line

- **Medical Hub:** ~2 features (contactless vitals *estimate*, fall/presence flag) + an LLM insight panel. **Not** a clinical monitoring system a hospital can use for care. Pi vitals are partly synthetic; nothing is validated or regulated; no patient, EHR, alarm, or nurse-call.
- **Security Center:** ~1–2 features (RF occupancy/presence + WhatsApp alert). **Not** a security system a hospital can use for intrusion/access. No access-control/VMS/central-station integration, single unmonitored channel, no zones/audit.
- **As a system:** a compelling **RF-sensing + management-backend platform / pilot**, but adoption as a hospital clinical or security *system* requires the integration + regulatory + reliability work above. Sell it as an **adjunct ambient-sensing layer**, not a turnkey hospital system.
