import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAuthStore } from "./auth-store";

export interface EnterpriseSettings {
  tenant_id: string;
  whapi_token?: string;
  whapi_number?: string;
  alert_target_number?: string;
  security_armed: boolean;
  crowd_threshold: number;
  fall_alert_enabled: boolean;
  vitals_alert_enabled: boolean;
  hr_min: number;
  hr_max: number;
  br_min: number;
  br_max: number;
}

export function useEnterpriseSettings() {
  const { accessToken } = useAuthStore();
  const [settings, setSettings] = useState<EnterpriseSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchSettings = async () => {
    if (!accessToken) return;
    setLoading(true);
    try {
      const res = await invoke<EnterpriseSettings>("get_enterprise_settings", {
        accessToken,
      });
      setSettings(res);
      setError(null);
    } catch (err) {
      setError(err as string);
    } finally {
      setLoading(false);
    }
  };

  const saveSettings = async (newSettings: EnterpriseSettings) => {
    if (!accessToken) return;
    try {
      await invoke("save_enterprise_settings", {
        accessToken,
        settings: newSettings,
      });
      setSettings(newSettings);
      return true;
    } catch (err) {
      setError(err as string);
      return false;
    }
  };

  const sendTestMessage = async () => {
    if (!accessToken) return;
    try {
      await invoke("whapi_send_test", { accessToken });
      return true;
    } catch (err) {
      setError(err as string);
      return false;
    }
  };

  const getWhapiQR = async () => {
    if (!accessToken) return;
    try {
      return await invoke<string>("whapi_get_qr", { accessToken });
    } catch (err) {
      setError(err as string);
      return null;
    }
  };

  useEffect(() => {
    fetchSettings();
  }, [accessToken]);

  return {
    settings,
    loading,
    error,
    saveSettings,
    refresh: fetchSettings,
    sendTestMessage,
    getWhapiQR,
  };
}
