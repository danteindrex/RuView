import React, { createContext, useEffect, useRef } from "react";
import { useSensingStore } from "./sensing-store";
import { tauriApi } from "./tauri-api";
import { useAuthStore } from "./auth-store";
import { useAlertSystem } from "./use-alert-system";

export const SensingContext = createContext({});

const STATUS_POLL_MS = 5000;

function resolveWsUrl(bindAddress: string | null, port: number): string {
  const host = bindAddress && bindAddress !== "0.0.0.0" ? bindAddress : "127.0.0.1";
  return `ws://${host}:${port}/ws/sensing`;
}

export const SensingProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const { ingestMessage, setConnected } = useSensingStore();
  const { accessToken } = useAuthStore();
  const wsRef = useRef<WebSocket | null>(null);
  const wsUrlRef = useRef<string | null>(null);

  // Start the Alert System hook
  useAlertSystem();

  useEffect(() => {
    const closeSocket = () => {
      const ws = wsRef.current;
      wsRef.current = null;
      wsUrlRef.current = null;
      if (ws) {
        try {
          ws.close();
        } catch {
          // ignore — socket may already be closed
        }
      }
      setConnected(false);
    };

    const connect = (url: string) => {
      // Close any existing socket before replacing it so stale connections
      // never linger alongside the new one.
      closeSocket();

      console.log(`[SensingProvider] Connecting to ${url}`);
      const ws = new WebSocket(url);
      wsRef.current = ws;
      wsUrlRef.current = url;

      ws.onopen = () => {
        if (wsRef.current !== ws) return;
        setConnected(true);
        console.log("[SensingProvider] WebSocket connected");
      };

      ws.onmessage = (event) => {
        if (wsRef.current !== ws) return;
        try {
          ingestMessage(JSON.parse(event.data));
        } catch (e) {
          console.error("[SensingProvider] Parse error", e);
        }
      };

      ws.onclose = () => {
        if (wsRef.current !== ws) return;
        wsRef.current = null;
        wsUrlRef.current = null;
        setConnected(false);
        console.log("[SensingProvider] WebSocket closed");
      };

      ws.onerror = (e) => {
        if (wsRef.current !== ws) return;
        console.error("[SensingProvider] WebSocket error", e);
      };
    };

    const checkStatus = async () => {
      try {
        if (!accessToken) return;
        const status = await tauriApi.serverStatus(accessToken);
        if (status.running && status.ws_port) {
          const url = resolveWsUrl(status.bind_address, status.ws_port);
          const ws = wsRef.current;
          const busy =
            ws !== null &&
            (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING);
          if (busy && wsUrlRef.current === url) {
            // Healthy (or still connecting) to the right endpoint — leave it alone.
            return;
          }
          // No usable socket, or ws_port/bind_address changed — (re)connect.
          connect(url);
        } else {
          closeSocket();
        }
      } catch (e) {
        console.error("Failed to check server status", e);
      }
    };

    void checkStatus();
    const interval = setInterval(() => void checkStatus(), STATUS_POLL_MS);

    return () => {
      clearInterval(interval);
      // The socket must not survive logout/unmount.
      closeSocket();
    };
  }, [accessToken, setConnected, ingestMessage]);

  return <SensingContext.Provider value={{}}>{children}</SensingContext.Provider>;
};
