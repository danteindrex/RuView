import { create } from "zustand";

// ── Feature Info ────────────────────────────────────────────────────────────

export interface WsFeatureInfo {
  mean_rssi: number;
  variance: number;
  motion_band_power: number;
  breathing_band_power: number;
  dominant_freq_hz: number;
  change_points: number;
  spectral_power: number;
}

// ── Signal Field (3D grid) ────────────────────────────────────────────────────

export interface WsSignalField {
  grid_size: [number, number, number];
  values: number[];
}

// ── Pose Keypoint (COCO 17-keypoint) ─────────────────────────────────────────

export interface WsPoseKeypoint {
  name: string;
  x: number;
  y: number;
  z: number;
  confidence: number;
}

// ── Bounding Box ─────────────────────────────────────────────────────────────

export interface WsBoundingBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

// ── Person Detection ─────────────────────────────────────────────────────────

export interface WsPersonDetection {
  id: number;
  confidence: number;
  keypoints: WsPoseKeypoint[];
  bbox: WsBoundingBox;
  zone: string;
}

// ── Per-Node Feature Info ────────────────────────────────────────────────────

export interface WsPerNodeFeatureInfo {
  node_id: number;
  features: WsFeatureInfo;
  classification: {
    motion_level: string;
    presence: boolean;
    confidence: number;
  };
  rssi_dbm: number;
  last_seen_ms: number;
  frame_rate_hz: number;
  stale: boolean;
}

// ── Node Info ────────────────────────────────────────────────────────────────

export interface WsNodeInfo {
  node_id: number;
  rssi_dbm: number;
  position: [number, number, number];
  amplitude: number[];
  subcarrier_count: number;
}

// ── Vital Signs ──────────────────────────────────────────────────────────────

export interface WsVitalSigns {
  breathing_rate_bpm?: number;
  heart_rate_bpm?: number;
  breathing_confidence?: number;
  heartbeat_confidence?: number;
  signal_quality?: number;
}

// ── Classification (matches Rust ClassificationInfo) ─────────────────────────

export interface WsClassification {
  motion_level: string;
  presence: boolean;
  confidence: number;
}

// ── Sensing Update (matches Rust SensingUpdate exactly) ───────────────────────

export interface WsSensingUpdate {
  type: string;
  timestamp: number;
  source: string;
  tick: number;
  nodes: WsNodeInfo[];
  features: WsFeatureInfo;
  classification: WsClassification;
  signal_field: WsSignalField;
  vital_signs?: WsVitalSigns;
  enhanced_motion?: unknown;
  enhanced_breathing?: unknown;
  /** PostureClass serialized as snake_case: "empty" | "standing" | "sitting" | "lying_down" | "walking" | "unknown". */
  posture?: string;
  signal_quality_score?: number;
  quality_verdict?: string;
  bssid_count?: number;
  pose_keypoints?: [number, number, number, number][]; // x,y,z,conf tuple
  model_status?: unknown;
  persons?: WsPersonDetection[];
  estimated_persons?: number;
  node_features?: WsPerNodeFeatureInfo[];
  /** Energy-weighted person location estimate in room coordinates [x, y] meters. */
  location_hint?: [number, number];
}

// ── Edge Vitals (WS "edge_vitals" message, ADR-039) ──────────────────────────

export interface WsEdgeVitals {
  type: "edge_vitals";
  node_id: number;
  presence?: boolean;
  fall_detected?: boolean;
  motion?: number;
  breathing_rate_bpm?: number;
  heartrate_bpm?: number;
  n_persons?: number;
  motion_energy?: number;
  presence_score?: number;
  rssi?: number;
  // mmWave-sourced fields (present when a 60 GHz node contributes them)
  heart_rate_bpm?: number;
  distance_cm?: number;
  distance_m?: number;
  /** Client-side receive timestamp (ms), stamped on ingest. */
  received_at?: number;
  [extra: string]: unknown;
}

// ── WASM Event (WS "wasm_event" message, ADR-040) ────────────────────────────

export interface WsWasmEvent {
  type: "wasm_event";
  node_id: number;
  module_id?: number;
  events?: unknown;
  /** Client-side receive timestamp (ms), stamped on ingest. */
  received_at?: number;
  [extra: string]: unknown;
}

// ── Feature State (WS "feature_state" message, per node) ────────────────────

export interface WsFeatureState {
  type: "feature_state";
  node_id: number;
  /** Client-side receive timestamp (ms), stamped on ingest. */
  received_at?: number;
  [extra: string]: unknown;
}

const MAX_WASM_EVENTS = 50;

interface SensingState {
  latestUpdate: WsSensingUpdate | null;
  /** Latest edge-vitals packet (per broadcast, not per node). */
  edgeVitals: WsEdgeVitals | null;
  /** Bounded list of the most recent WASM runtime events (newest last). */
  wasmEvents: WsWasmEvent[];
  /** Latest feature-state message per node_id. */
  featureStates: Record<number, WsFeatureState>;
  connected: boolean;
  /**
   * Route a raw WebSocket message by its "type" field. Only "sensing_update"
   * touches latestUpdate — other message types must never clobber it (doing so
   * blanked the dashboard between sensing updates).
   */
  ingestMessage: (raw: unknown) => void;
  setUpdate: (update: WsSensingUpdate) => void;
  setConnected: (connected: boolean) => void;
}

export const useSensingStore = create<SensingState>((set) => ({
  latestUpdate: null,
  edgeVitals: null,
  wasmEvents: [],
  featureStates: {},
  connected: false,
  ingestMessage: (raw) => {
    if (!raw || typeof raw !== "object") return;
    const msg = raw as { type?: unknown; node_id?: unknown };
    switch (msg.type) {
      case "sensing_update":
        set({ latestUpdate: raw as WsSensingUpdate });
        break;
      case "edge_vitals":
        set({ edgeVitals: { ...(raw as WsEdgeVitals), received_at: Date.now() } });
        break;
      case "wasm_event":
        set((state) => ({
          wasmEvents: [
            ...state.wasmEvents.slice(-(MAX_WASM_EVENTS - 1)),
            { ...(raw as WsWasmEvent), received_at: Date.now() },
          ],
        }));
        break;
      case "feature_state": {
        if (typeof msg.node_id !== "number") return;
        const nodeId = msg.node_id;
        set((state) => ({
          featureStates: {
            ...state.featureStates,
            [nodeId]: { ...(raw as WsFeatureState), received_at: Date.now() },
          },
        }));
        break;
      }
      default:
        // Unknown/untyped frames: accept as a sensing update only when they
        // carry the sensing payload shape (legacy servers omitted "type").
        if (msg.type === undefined && (raw as WsSensingUpdate).classification) {
          set({ latestUpdate: raw as WsSensingUpdate });
        }
        break;
    }
  },
  setUpdate: (update) => set({ latestUpdate: update }),
  setConnected: (connected) => set({ connected }),
}));
