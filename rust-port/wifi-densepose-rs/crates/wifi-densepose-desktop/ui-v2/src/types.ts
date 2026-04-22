export type HealthStatus = "online" | "offline" | "degraded";
export type DiscoveryMethod = "mdns" | "udp_probe" | "http_sweep" | "manual";
export type Chip = "esp32" | "esp32s2" | "esp32s3" | "esp32c3" | "esp32c6";
export type MeshRole = "coordinator" | "node" | "aggregator";
export type DataSource = "auto" | "wifi" | "esp32" | "simulate";

export interface NodeCapabilities {
  wasm: boolean;
  ota: boolean;
  csi: boolean;
}

export interface DiscoveredNode {
  ip: string;
  mac: string | null;
  hostname: string | null;
  node_id: number;
  firmware_version: string | null;
  health: HealthStatus;
  last_seen: string;
  chip: Chip;
  mesh_role: MeshRole;
  discovery_method: DiscoveryMethod;
  tdm_slot: number | null;
  tdm_total: number | null;
  edge_tier: number | null;
  uptime_secs: number | null;
  capabilities: NodeCapabilities | null;
  friendly_name: string | null;
  notes: string | null;
}

export interface SerialPortInfo {
  name: string;
  vid: number | null;
  pid: number | null;
  manufacturer: string | null;
  serial_number: string | null;
  is_esp32_compatible: boolean;
}

export interface FlashProgress {
  phase: string;
  progress_pct: number;
  bytes_written: number;
  bytes_total: number;
  message: string | null;
}

export interface FlashResult {
  success: boolean;
  message: string;
  duration_secs: number;
  firmware_hash: string | null;
}

export interface VerifyResult {
  verified: boolean;
  expected_hash: string;
  actual_hash: string | null;
  message: string;
}

export interface EspflashInfo {
  installed: boolean;
  version: string | null;
  path: string | null;
}

export interface ChipInfo {
  id: string;
  name: string;
  description: string;
}

export interface OtaResult {
  success: boolean;
  node_ip: string;
  message: string;
  firmware_hash: string | null;
  duration_secs: number | null;
}

export interface OtaEndpointInfo {
  reachable: boolean;
  ota_supported: boolean;
  current_version: string | null;
  psk_required: boolean;
}

export interface BatchOtaResult {
  total: number;
  completed: number;
  failed: number;
  results: OtaResult[];
  duration_secs: number;
}

export interface WasmModuleInfo {
  id: string;
  name: string;
  size_bytes: number;
  status: string;
  sha256: string | null;
  loaded_at: string | null;
  memory_used_kb: number | null;
  cpu_usage_pct: number | null;
  exec_count: number | null;
}

export interface WasmUploadResult {
  success: boolean;
  module_id: string;
  message: string;
  sha256: string | null;
}

export interface WasmControlResult {
  success: boolean;
  module_id: string;
  action: string;
  message: string;
}

export interface WasmModuleDetail {
  id: string;
  name: string;
  size_bytes: number;
  status: string;
  sha256: string;
  loaded_at: string;
  memory_used_kb: number;
  exports: string[];
  imports: string[];
  execution_count: number;
  last_error: string | null;
}

export interface WasmRuntimeStats {
  total_modules: number;
  running_modules: number;
  memory_used_kb: number;
  memory_limit_kb: number;
  total_executions: number;
  errors: number;
}

export interface WasmSupportInfo {
  supported: boolean;
  max_modules: number | null;
  memory_limit_kb: number | null;
  verify_signatures: boolean;
}

export interface ServerConfig {
  http_port?: number | null;
  ws_port?: number | null;
  udp_port?: number | null;
  log_level?: string | null;
  bind_address?: string | null;
  server_path?: string | null;
  source?: string | null;
}

export interface ServerStartResult {
  pid: number;
  http_port: number | null;
  ws_port: number | null;
  udp_port: number | null;
}

export interface ServerStatusResponse {
  running: boolean;
  pid: number | null;
  http_port: number | null;
  ws_port: number | null;
  udp_port: number | null;
  memory_mb: number | null;
  cpu_percent: number | null;
  uptime_secs: number | null;
}

export interface ServerLogsResponse {
  stdout: string[];
  stderr: string[];
  truncated: boolean;
}

export interface ProvisioningConfig {
  wifi_ssid?: string | null;
  wifi_password?: string | null;
  target_ip?: string | null;
  target_port?: number | null;
  node_id?: number | null;
  tdm_slot?: number | null;
  tdm_total?: number | null;
  edge_tier?: number | null;
  presence_thresh?: number | null;
  fall_thresh?: number | null;
  vital_window?: number | null;
  vital_interval_ms?: number | null;
  top_k_count?: number | null;
  hop_count?: number | null;
  channel_list?: number[] | null;
  dwell_ms?: number | null;
  power_duty?: number | null;
  wasm_max_modules?: number | null;
  wasm_verify?: boolean | null;
  ota_psk?: string | null;
}

export interface ProvisionResult {
  success: boolean;
  message: string;
  checksum: string | null;
}

export interface ValidationResult {
  valid: boolean;
  message: string | null;
  estimated_size: number;
}

export interface MeshNodeConfig {
  node_id: number;
  tdm_slot: number;
  config: ProvisioningConfig;
}

export interface AppSettings {
  server_http_port: number;
  server_ws_port: number;
  server_udp_port: number;
  bind_address: string;
  ui_path: string;
  ota_psk: string;
  auto_discover: boolean;
  discover_interval_ms: number;
  theme: "light" | "dark" | string;
}

export interface PoseLandmark {
  x: number;
  y: number;
  z: number;
  visibility?: number;
  presence?: number;
}

