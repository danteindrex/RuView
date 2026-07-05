import { useEffect, useRef } from "react";
import { useSensingStore } from "./sensing-store";
import { useEnterpriseSettings } from "./enterprise-store";
import { invoke } from "@tauri-apps/api/core";
import { useAuthStore } from "./auth-store";

/**
 * useAlertSystem — The "Active Logic Engine" for RuView Enterprise.
 * 
 * Monitors the real-time sensing stream and dispatches WhatsApp notifications
 * based on tenant-specific thresholds (Fall, Vitals, Intrusion, Crowd).
 */
export function useAlertSystem() {
  const { latestUpdate } = useSensingStore();
  const { settings } = useEnterpriseSettings();
  const { accessToken } = useAuthStore();
  
  // Cooldown tracking to prevent message spamming
  const lastAlertTime = useRef<Record<string, number>>({});
  const ALERT_COOLDOWN_MS = 60000; // 1 minute

  useEffect(() => {
    if (!latestUpdate || !settings || !accessToken) return;

    const now = Date.now();

    const dispatchAlert = async (type: string, message: string) => {
      const lastTime = lastAlertTime.current[type] || 0;
      if (now - lastTime < ALERT_COOLDOWN_MS) return;

      console.log(`[AlertSystem] 🚨 Triggering ${type}: ${message}`);
      
      try {
        await invoke("whapi_send_alert", { 
          accessToken, 
          message 
        });
        lastAlertTime.current[type] = now;
      } catch (err) {
        console.error(`[AlertSystem] Failed to send ${type} alert:`, err);
      }
    };

    // --- 1. MEDICAL: Fall Detection (via posture field) ---
    // The Rust sensing-server serializes PostureClass as snake_case strings:
    // "empty" | "standing" | "sitting" | "lying_down" | "walking" | "unknown".
    // There is no dedicated "fallen" variant — "lying_down" is the fall-relevant
    // posture. (fall_detected does NOT exist in the WebSocket SensingUpdate.)
    if (settings.fall_alert_enabled && latestUpdate.posture === "lying_down") {
      dispatchAlert("fall", "🚨 [MEDICAL] FALL DETECTED! A patient may need urgent assistance.");
    }

    // --- 2. MEDICAL: Abnormal Vitals ---
    if (settings.vitals_alert_enabled && latestUpdate.vital_signs) {
      const hr = latestUpdate.vital_signs.heart_rate_bpm;
      const br = latestUpdate.vital_signs.breathing_rate_bpm;

      if (hr && (hr > settings.hr_max || hr < settings.hr_min)) {
        dispatchAlert("hr_anomaly", `⚠️ [MEDICAL] HEART RATE ANOMALY: ${hr.toFixed(0)} BPM detected (Range: ${settings.hr_min}-${settings.hr_max}).`);
      }
      if (br && (br > settings.br_max || br < settings.br_min)) {
        dispatchAlert("br_anomaly", `⚠️ [MEDICAL] RESPIRATORY ANOMALY: ${br.toFixed(1)} BPM detected (Range: ${settings.br_min}-${settings.br_max}).`);
      }
    }

    // --- 3. SECURITY: Intrusion (Armed Mode) ---
    if (settings.security_armed && latestUpdate.classification?.presence) {
      dispatchAlert("intrusion", "🛡️ [SECURITY] INTRUSION DETECTED: Movement recognized while system is ARMED.");
    }

    // --- 4. SECURITY: Crowd Density ---
    if (latestUpdate.estimated_persons && latestUpdate.estimated_persons > settings.crowd_threshold) {
      dispatchAlert("crowd", `👥 [SECURITY] CAPACITY EXCEEDED: ${latestUpdate.estimated_persons} people detected (Limit: ${settings.crowd_threshold}).`);
    }

  }, [latestUpdate, settings, accessToken]);
}
