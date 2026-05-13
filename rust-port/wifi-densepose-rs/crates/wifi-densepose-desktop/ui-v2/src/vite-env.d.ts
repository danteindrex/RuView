/// <reference types="vite/client" />

interface Window {
  /** Tauri IPC bridge — injected by the Rust webview before scripts run. */
  __TAURI_INTERNALS__?: unknown;
}

declare module "/observatory/js/main.js" {
  export function mountObservatory(
    root: HTMLElement,
    options?: { mode?: "live" | "demo"; wsUrl?: string | null },
  ): { destroy?: () => void };
}
