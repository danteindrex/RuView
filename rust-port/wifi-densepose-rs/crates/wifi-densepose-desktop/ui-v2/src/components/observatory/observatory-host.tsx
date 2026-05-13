import React, { useEffect, useRef } from "react";
import { ObservatoryMarkup } from "./observatory-markup";

const OBSERVATORY_CSS_ID = "wave-observatory-css";
const OBSERVATORY_IMPORTMAP_ID = "wave-observatory-importmap";

interface ObservatoryHostProps {
  mode: "live" | "demo";
  theme: "light" | "dark";
  wsUrl: string | null;
}

function ensureObservatoryCss() {
  if (document.getElementById(OBSERVATORY_CSS_ID)) return;
  const link = document.createElement("link");
  link.id = OBSERVATORY_CSS_ID;
  link.rel = "stylesheet";
  link.href = "/observatory/css/observatory.css";
  document.head.appendChild(link);
}

function ensureObservatoryImportMap() {
  if (document.getElementById(OBSERVATORY_IMPORTMAP_ID)) return;
  const script = document.createElement("script");
  script.id = OBSERVATORY_IMPORTMAP_ID;
  script.type = "importmap";
  script.textContent = JSON.stringify({
    imports: {
      three: "https://unpkg.com/three@0.160.0/build/three.module.js",
      "three/addons/": "https://unpkg.com/three@0.160.0/examples/jsm/",
    },
  });
  document.head.appendChild(script);
}

export function ObservatoryHost({ mode, theme, wsUrl }: ObservatoryHostProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let mountedInstance: { destroy?: () => void } | null = null;
    let cancelled = false;

    ensureObservatoryCss();
    ensureObservatoryImportMap();

    // Inject a script tag to load the module, bypassing Vite's strict import analysis
    if (!(window as any).__WAVE_OBSERVATORY__) {
      const script = document.createElement("script");
      script.type = "module";
      script.textContent = `
        import * as obs from "/observatory/js/main.js";
        window.__WAVE_OBSERVATORY__ = obs;
        window.dispatchEvent(new Event('observatory_loaded'));
      `;
      document.head.appendChild(script);
    }

    const initModule = () => {
      const module = (window as any).__WAVE_OBSERVATORY__;
      if (cancelled || !rootRef.current || typeof module?.mountObservatory !== "function") {
        return;
      }
      mountedInstance = module.mountObservatory(rootRef.current, { mode, wsUrl });
    };

    if ((window as any).__WAVE_OBSERVATORY__) {
      initModule();
    } else {
      window.addEventListener('observatory_loaded', initModule, { once: true });
    }

    return () => {
      cancelled = true;
      mountedInstance?.destroy?.();
    };
  }, [mode, wsUrl]);

  return (
    <div
      ref={rootRef}
      className="observatory-root"
      data-theme={theme}
      data-observatory-managed="react"
    >
      <ObservatoryMarkup />
    </div>
  );
}
