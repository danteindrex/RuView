/**
 * Onboarding Store — first-run wizard visibility + step machine (W3).
 *
 * The wizard is driven from three places: App.tsx launch-gating, the Settings
 * re-open entry, and the wizard's own Back/Next/Skip controls. A tiny zustand
 * store keeps that state in one place instead of prop-drilling through the shell.
 *
 * Completion is persisted via `save_settings` (AppSettings.onboarding_complete)
 * AND mirrored to localStorage as a fallback, because an older backend may drop
 * the unknown field — without the mirror the wizard would reopen every launch.
 */

import { create } from "zustand";
import { tauriApi } from "./tauri-api";
import type { AppSettings } from "@/types";

export const ONBOARDING_STEPS = [
  "welcome",
  "hub",
  "nodes",
  "place",
  "calibrate",
  "tour",
] as const;

export type OnboardingStep = (typeof ONBOARDING_STEPS)[number];

/** Total number of steps in the wizard. */
export const STEP_COUNT = ONBOARDING_STEPS.length;

const LS_COMPLETE_KEY = "wave-onboarding-complete";

/** Tour target the wizard wants the app to navigate to (App.tsx honors this). */
export type TourPage = "dashboard" | "pose3d" | "medical" | null;

interface OnboardingState {
  open: boolean;
  /** Current step index into ONBOARDING_STEPS. */
  step: number;
  /** Page the tour step wants App.tsx to show (null = leave navigation alone). */
  tourPage: TourPage;

  openAt: (step: number) => void;
  close: () => void;
  next: () => void;
  back: () => void;
  goTo: (step: number) => void;
  setTourPage: (page: TourPage) => void;
}

export const useOnboardingStore = create<OnboardingState>((set) => ({
  open: false,
  step: 0,
  tourPage: null,

  openAt: (step) => set({ open: true, step: clampStep(step) }),
  close: () => set({ open: false, tourPage: null }),
  next: () => set((s) => ({ step: clampStep(s.step + 1) })),
  back: () => set((s) => ({ step: clampStep(s.step - 1) })),
  goTo: (step) => set({ step: clampStep(step) }),
  setTourPage: (page) => set({ tourPage: page }),
}));

function clampStep(step: number): number {
  if (Number.isNaN(step)) return 0;
  return Math.max(0, Math.min(STEP_COUNT - 1, step));
}

/** Read the localStorage mirror, tolerating environments where access throws. */
function readCompleteMirror(): boolean {
  try {
    return localStorage.getItem(LS_COMPLETE_KEY) === "true";
  } catch {
    return false;
  }
}

/** True if onboarding is already complete per settings OR the localStorage mirror. */
export function isOnboardingComplete(settings: AppSettings | null): boolean {
  if (settings?.onboarding_complete === true) return true;
  return readCompleteMirror();
}

/**
 * Persist completion. Mirrors to localStorage immediately (so it survives a
 * backend that drops the field), then best-effort writes it into AppSettings.
 */
export async function persistOnboardingComplete(accessToken: string | null): Promise<void> {
  try {
    localStorage.setItem(LS_COMPLETE_KEY, "true");
  } catch {
    // Storage unavailable — the settings write below is the durable path.
  }
  if (!accessToken) return;
  try {
    const current = await tauriApi.getSettings(accessToken);
    if (current) {
      await tauriApi.saveSettings(accessToken, { ...current, onboarding_complete: true });
    }
  } catch {
    // Backend unavailable / older schema — the localStorage mirror is enough.
  }
}
