/**
 * Sensing-server HTTP helpers for the onboarding wizard (W3).
 *
 * The sensing server is the local Axum process the observatory already talks
 * to. Its `/api/v1/*` routes are tokenless on the loopback interface (matching
 * how the observatory front-end fetches), so these helpers do plain fetches.
 *
 * Base URL is derived from `server_status` (http_port + bind_address), exactly
 * as SensingProvider / pose3d derive the WS URL. On this dev machine the port
 * is 8090 (taha-backend owns 8080), so we never hard-code 8080.
 */

import { isLoopbackHostPort } from "@/lib/utils";
import type { CalibrationStatus, Chip, DiscoveredNode, ServerNode, ServerStatusResponse } from "@/types";

/**
 * The hub's real LAN IP for prefilling node targets/aggregators, or "" when the
 * bind address is only loopback / all-interfaces (a node must stream to a
 * reachable LAN address, never loopback or 0.0.0.0).
 */
export function deriveHubLanIp(status: ServerStatusResponse | null): string {
  const bind = status?.bind_address ?? "";
  if (!bind || bind === "0.0.0.0" || isLoopbackHostPort(bind)) return "";
  return bind;
}

/**
 * Client-reachable host for a server bind address. A `0.0.0.0` (all-interfaces)
 * bind isn't a connectable client host, so fall back to loopback.
 */
export function resolveClientHost(bindAddress: string | null | undefined): string {
  return bindAddress && bindAddress !== "0.0.0.0" ? bindAddress : "127.0.0.1";
}

/** Resolve `http://host:port` for the running sensing server, or null. */
export function resolveServerBase(status: ServerStatusResponse | null): string | null {
  if (!status?.running || !status.http_port) return null;
  return `http://${resolveClientHost(status.bind_address)}:${status.http_port}`;
}

/**
 * Resolve the sensing WebSocket URL, or null when the WS port is unknown so the
 * observatory's own auto-detect (its :8765/:8080 fallback) runs instead of
 * pinning a dead port.
 */
export function resolveSensingWsUrl(status: ServerStatusResponse | null): string | null {
  if (!status?.ws_port) return null;
  return `ws://${resolveClientHost(status.bind_address)}:${status.ws_port}/ws/sensing`;
}

async function getJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const res = await fetch(url, { signal });
  if (!res.ok) {
    throw new HttpError(res.status, `${res.status} ${res.statusText}`);
  }
  return (await res.json()) as T;
}

/** Error carrying the HTTP status so callers can special-case 404 (port conflict). */
export class HttpError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
    this.name = "HttpError";
  }
}

/** GET /health — returns true if the server answers 2xx. */
export async function probeHealth(base: string, signal?: AbortSignal): Promise<boolean> {
  try {
    const res = await fetch(`${base}/health`, { signal });
    return res.ok;
  } catch {
    return false;
  }
}

/** GET /api/v1/nodes — the node roster the observatory streams. */
export async function fetchServerNodes(base: string, signal?: AbortSignal): Promise<ServerNode[]> {
  const data = await getJson<{ nodes?: ServerNode[] } | ServerNode[]>(`${base}/api/v1/nodes`, signal);
  if (Array.isArray(data)) return data;
  return data.nodes ?? [];
}

/**
 * GET /api/v1/config/node-positions — the persisted placement map, keyed by
 * node id → [x,y,z]. Manual drags PUT here (not into /api/v1/nodes), so the
 * placement step must consult this to know a node has been placed.
 */
export async function fetchNodePositions(base: string, signal?: AbortSignal): Promise<Record<string, unknown>> {
  const data = await getJson<{ positions?: Record<string, unknown> } | null>(`${base}/api/v1/config/node-positions`, signal);
  return data?.positions ?? {};
}

/** True when a value is a finite 3-tuple [x,y,z]. */
export function isPosition3(value: unknown): value is [number, number, number] {
  return Array.isArray(value) && value.length >= 3 && value.slice(0, 3).every((v) => Number.isFinite(v));
}

/** POST /api/v1/calibration/start?duration_secs=N */
export async function startCalibration(base: string, durationSecs: number): Promise<void> {
  const res = await fetch(`${base}/api/v1/calibration/start?duration_secs=${durationSecs}`, { method: "POST" });
  if (!res.ok) throw new HttpError(res.status, `calibration start failed: ${res.status}`);
}

/** POST /api/v1/calibration/stop */
export async function stopCalibration(base: string): Promise<void> {
  const res = await fetch(`${base}/api/v1/calibration/stop`, { method: "POST" });
  if (!res.ok) throw new HttpError(res.status, `calibration stop failed: ${res.status}`);
}

/** GET /api/v1/calibration/status */
export async function fetchCalibrationStatus(base: string, signal?: AbortSignal): Promise<CalibrationStatus> {
  return getJson<CalibrationStatus>(`${base}/api/v1/calibration/status`, signal);
}

/** Normalize a node id to a number where possible (server may send string ids). */
export function nodeIdNumber(node: ServerNode): number | null {
  const n = typeof node.node_id === "number" ? node.node_id : Number(node.node_id);
  return Number.isFinite(n) ? n : null;
}

/**
 * True when a node is live. Matches the observatory's own rule
 * (`(status || 'active') === 'active'`): the server defaults an omitted status
 * to "active", and marks position-only / stale entries "offline". So an empty
 * status counts as online, but an explicit "offline" does not.
 */
export function isNodeOnline(node: ServerNode): boolean {
  const s = (node.status ?? "active").toLowerCase();
  return s === "active" || s === "online" || s === "streaming";
}

/**
 * Merge the sensing-server streaming roster (`/api/v1/nodes`) into the Tauri
 * discovery list so the whole app agrees on which nodes exist and are online.
 *
 * Discovery (`discoverNodes`) probes the network and carries IP/MAC/chip needed
 * for flashing & OTA, but ESP32/Pi nodes stream to the aggregator without
 * answering the probe — so discovery alone reports 0 while data flows. The
 * roster is the source of truth for "online", so we:
 *  - overlay roster status onto matching discovered nodes (by node_id), and
 *  - append synthesized entries for roster nodes discovery never found,
 * marking them online so the header/dashboard/mesh counts reflect reality.
 */
export function mergeRosterIntoNodes(
  discovered: DiscoveredNode[],
  roster: ServerNode[],
): DiscoveredNode[] {
  const rosterById = new Map<number, ServerNode>();
  for (const r of roster) {
    const id = nodeIdNumber(r);
    if (id !== null) rosterById.set(id, r);
  }

  // 1. Overlay roster "online" onto matching discovered nodes.
  const merged: DiscoveredNode[] = discovered.map((d) => {
    const r = rosterById.get(d.node_id);
    if (r && isNodeOnline(r)) return { ...d, health: "online" };
    return d;
  });

  // 2. Append roster nodes discovery never found (streaming-only).
  const discoveredIds = new Set(discovered.map((d) => d.node_id));
  for (const r of roster) {
    const id = nodeIdNumber(r);
    if (id === null || discoveredIds.has(id)) continue;
    // Infer chip class from the producing parser: nexmon ⇒ Pi (BCM43455).
    const isNexmon = (r.origin ?? "").toLowerCase().includes("nexmon");
    const chip: Chip = isNexmon ? "esp32" : "esp32s3";
    merged.push({
      ip: "",
      mac: null,
      hostname: null,
      node_id: id,
      firmware_version: null,
      health: isNodeOnline(r) ? "online" : "offline",
      last_seen: new Date().toISOString(),
      chip,
      mesh_role: "node",
      discovery_method: "udp_probe",
      tdm_slot: null,
      tdm_total: null,
      edge_tier: null,
      uptime_secs: null,
      capabilities: null,
      friendly_name: null,
      notes: isNexmon ? "Streaming (Nexmon/Pi)" : "Streaming (ESP32)",
    });
  }
  return merged;
}
