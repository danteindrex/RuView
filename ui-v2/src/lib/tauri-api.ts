import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings,
  BatchOtaResult,
  ChipInfo,
  DiscoveredNode,
  Esp32ProvisionConfig,
  Esp32SerialCheck,
  EspflashInfo,
  FirmwareRelease,
  FlashProgress,
  FlashResult,
  FlashSegment,
  MeshNodeConfig,
  OtaEndpointInfo,
  OtaResult,
  PiAgentConfig,
  PiNodeBuildRequest,
  PiNodeCommandResult,
  PiNodeTarget,
  PiServiceAction,
  ProvisionResult,
  ProvisioningConfig,
  HostNetworkInfo,
  SerialPortInfo,
  WifiNetwork,
  ServerConfig,
  ServerLogsResponse,
  ServerStartResult,
  ServerStatusResponse,
  ValidationResult,
  VerifyResult,
  WasmControlResult,
  WasmModuleDetail,
  WasmModuleInfo,
  WasmRuntimeStats,
  WasmSupportInfo,
  WasmUploadResult,
} from "@/types";

async function invokeTauri<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export const tauriApi = {
  discoverNodes(accessToken: string, timeoutMs = 3000) {
    return invokeTauri<DiscoveredNode[]>("discover_nodes", { accessToken, timeoutMs });
  },
  listSerialPorts(accessToken: string) {
    return invokeTauri<SerialPortInfo[]>("list_serial_ports", { accessToken });
  },
  hostNetworkInfo(accessToken: string) {
    return invokeTauri<HostNetworkInfo>("host_network_info", { accessToken });
  },
  scanWifiNetworks(accessToken: string) {
    return invokeTauri<WifiNetwork[]>("scan_wifi_networks", { accessToken });
  },
  wifiSavedPassword(accessToken: string, ssid: string) {
    return invokeTauri<string | null>("wifi_saved_password", { accessToken, ssid });
  },
  configureEsp32Wifi(accessToken: string, port: string, ssid: string, password: string) {
    return invokeTauri<string>("configure_esp32_wifi", { accessToken, port, ssid, password });
  },
  flashFirmware(accessToken: string, params: { port: string; firmwarePath: string; chip?: string; baud?: number }) {
    return invokeTauri<FlashResult>("flash_firmware", { accessToken, ...params });
  },
  flashProgress(accessToken: string) {
    return invokeTauri<FlashProgress>("flash_progress", { accessToken });
  },
  verifyFirmware(accessToken: string, params: { port: string; firmwarePath: string; chip?: string }) {
    return invokeTauri<VerifyResult>("verify_firmware", { accessToken, ...params });
  },
  checkEspflash(accessToken: string) {
    return invokeTauri<EspflashInfo>("check_espflash", { accessToken });
  },
  supportedChips(accessToken: string) {
    return invokeTauri<ChipInfo[]>("supported_chips", { accessToken });
  },
  otaUpdate(accessToken: string, params: { nodeIp: string; firmwarePath: string; psk?: string }) {
    return invokeTauri<OtaResult>("ota_update", { accessToken, ...params });
  },
  batchOtaUpdate(accessToken: string, params: {
    nodeIps: string[];
    firmwarePath: string;
    psk?: string;
    strategy?: string;
    maxConcurrent?: number;
  }) {
    return invokeTauri<BatchOtaResult>("batch_ota_update", { accessToken, ...params });
  },
  checkOtaEndpoint(accessToken: string, nodeIp: string) {
    return invokeTauri<OtaEndpointInfo>("check_ota_endpoint", { accessToken, nodeIp });
  },
  wasmList(accessToken: string, nodeIp: string) {
    return invokeTauri<WasmModuleInfo[]>("wasm_list", { accessToken, nodeIp });
  },
  wasmUpload(accessToken: string, params: { nodeIp: string; wasmPath: string; moduleName?: string; autoStart?: boolean }) {
    return invokeTauri<WasmUploadResult>("wasm_upload", { accessToken, ...params });
  },
  wasmControl(accessToken: string, params: { nodeIp: string; moduleId: string; action: string }) {
    return invokeTauri<WasmControlResult>("wasm_control", { accessToken, ...params });
  },
  wasmInfo(accessToken: string, params: { nodeIp: string; moduleId: string }) {
    return invokeTauri<WasmModuleDetail>("wasm_info", { accessToken, ...params });
  },
  wasmStats(accessToken: string, nodeIp: string) {
    return invokeTauri<WasmRuntimeStats>("wasm_stats", { accessToken, nodeIp });
  },
  checkWasmSupport(accessToken: string, nodeIp: string) {
    return invokeTauri<WasmSupportInfo>("check_wasm_support", { accessToken, nodeIp });
  },
  startServer(accessToken: string, config: ServerConfig) {
    return invokeTauri<ServerStartResult>("start_server", { accessToken, config });
  },
  stopServer(accessToken: string) {
    return invokeTauri<void>("stop_server", { accessToken });
  },
  serverStatus(accessToken: string) {
    return invokeTauri<ServerStatusResponse>("server_status", { accessToken });
  },
  restartServer(accessToken: string, config?: ServerConfig) {
    return invokeTauri<ServerStartResult>("restart_server", { accessToken, config });
  },
  serverLogs(accessToken: string, lines?: number) {
    return invokeTauri<ServerLogsResponse>("server_logs", { accessToken, lines });
  },
  piNodeProbe(accessToken: string, target: PiNodeTarget) {
    return invokeTauri<PiNodeCommandResult>("pi_node_probe", { accessToken, target });
  },
  piNodeBuildAgent(accessToken: string, request: PiNodeBuildRequest) {
    return invokeTauri<PiNodeCommandResult>("pi_node_build_agent", { accessToken, request });
  },
  piNodeDeployBinary(accessToken: string, params: { target: PiNodeTarget; localBinaryPath: string; remoteBinaryPath?: string }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_deploy_binary", {
      accessToken,
      request: {
        target: params.target,
        local_binary_path: params.localBinaryPath,
        remote_binary_path: params.remoteBinaryPath,
      },
    });
  },
  piNodeCheckPrereqs(accessToken: string, params: { target: PiNodeTarget; installPackages?: boolean }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_check_prereqs", {
      accessToken,
      request: { target: params.target, install_packages: params.installPackages },
    });
  },
  piNodeCsiHealth(accessToken: string, params: { target: PiNodeTarget; nexmonPort?: number; captureSeconds?: number; serviceName?: string }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_csi_health", {
      accessToken,
      request: {
        target: params.target,
        nexmon_port: params.nexmonPort,
        capture_seconds: params.captureSeconds,
        service_name: params.serviceName,
      },
    });
  },
  piNodePushConfig(accessToken: string, params: { target: PiNodeTarget; config: PiAgentConfig; envPath?: string }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_push_config", {
      accessToken,
      request: { target: params.target, config: params.config, env_path: params.envPath },
    });
  },
  piNodeInstallService(accessToken: string, params: {
    target: PiNodeTarget;
    config: PiAgentConfig;
    serviceName?: string;
    binaryPath?: string;
    envPath?: string;
  }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_install_service", {
      accessToken,
      request: {
        target: params.target,
        config: params.config,
        service_name: params.serviceName,
        binary_path: params.binaryPath,
        env_path: params.envPath,
      },
    });
  },
  piNodeService(accessToken: string, params: { target: PiNodeTarget; action: PiServiceAction; serviceName?: string }) {
    return invokeTauri<PiNodeCommandResult>("pi_node_service", {
      accessToken,
      request: { target: params.target, action: params.action, service_name: params.serviceName },
    });
  },
  /** Install Nexmon CSI on the Pi end-to-end (long-running; ~20-40 min, reboots
   *  the Pi). Progress streams on the `pi-nexmon-progress` event — subscribe
   *  with `onPiNexmonProgress` before calling this. Resolves when Nexmon is ready. */
  piNodeInstallNexmon(accessToken: string, target: PiNodeTarget) {
    return invokeTauri<PiNodeCommandResult>("pi_node_install_nexmon", {
      accessToken,
      request: { target },
    });
  },
  /** Subscribe to Nexmon-install log lines. Returns an unlisten function. */
  async onPiNexmonProgress(handler: (line: string) => void): Promise<() => void> {
    return listen<{ line: string }>("pi-nexmon-progress", (event) => {
      handler(event.payload?.line ?? "");
    });
  },
  provisionNode(accessToken: string, port: string, config: ProvisioningConfig) {
    return invokeTauri<ProvisionResult>("provision_node", { accessToken, port, config });
  },
  readNvs(accessToken: string, port: string) {
    return invokeTauri<ProvisioningConfig>("read_nvs", { accessToken, port });
  },
  eraseNvs(accessToken: string, port: string) {
    return invokeTauri<ProvisionResult>("erase_nvs", { accessToken, port });
  },
  validateConfig(accessToken: string, config: ProvisioningConfig) {
    return invokeTauri<ValidationResult>("validate_config", { accessToken, config });
  },
  generateMeshConfigs(accessToken: string, baseConfig: ProvisioningConfig, nodeCount: number) {
    return invokeTauri<MeshNodeConfig[]>("generate_mesh_configs", { accessToken, baseConfig, nodeCount });
  },
  // ─── First-run onboarding wizard (W3) — sibling-branch commands ───────────
  // These land when W1's branch merges; call sites guard defensively so the
  // build (and no-backend smoke test) still pass when they are absent.
  fetchFirmwareRelease(accessToken: string, tag?: string) {
    return invokeTauri<FirmwareRelease>("fetch_firmware_release", { accessToken, tag });
  },
  flashFirmwareBundle(accessToken: string, params: { port: string; chip: string; segments: FlashSegment[]; baud?: number }) {
    return invokeTauri<FlashResult>("flash_firmware_bundle", { accessToken, ...params });
  },
  provisionEsp32Nvs(accessToken: string, params: { port: string; chip: string; config: Esp32ProvisionConfig; baud?: number }) {
    return invokeTauri<ProvisionResult>("provision_esp32_nvs", { accessToken, ...params });
  },
  /** After provisioning, read the node's serial console to confirm it joined
   *  WiFi (resets the board and scans the boot log for "Connected to WiFi").
   *  Splits a "watching forever" failure into a concrete cause. */
  esp32SerialCheck(accessToken: string, port: string, timeoutSecs = 30) {
    return invokeTauri<Esp32SerialCheck>("esp32_serial_check", { accessToken, port, timeoutSecs });
  },
  getSettings(accessToken: string) {
    return invokeTauri<AppSettings | null>("get_settings", { accessToken });
  },
  saveSettings(accessToken: string, settings: AppSettings) {
    return invokeTauri<void>("save_settings", { accessToken, settings });
  },
  setSshKey(keyPem: string, passphrase: string) {
    return invokeTauri<void>("set_ssh_key", { keyPem, passphrase });
  },
  hasSshKey() {
    return invokeTauri<boolean>("has_ssh_key");
  },
  setLangfuseConfig(config: { enabled: boolean; host: string; public_key: string; secret_key: string }) {
    return invokeTauri<void>("set_langfuse_config", { config });
  },
  getLangfuseConfig() {
    return invokeTauri<{ enabled: boolean; host: string; public_key_hint: string }>("get_langfuse_config");
  },
  getDeploymentInfo() {
    return invokeTauri<{ deployment_id: string; deployment_name: string; location_name: string; latitude: number | null; longitude: number | null; tenant_id: string | null }>("get_deployment_info");
  },
  setDeploymentInfo(info: { deploymentName: string; locationName: string; latitude?: number; longitude?: number; tenantId?: string }) {
    return invokeTauri<void>("set_deployment_info", {
      deploymentName: info.deploymentName,
      locationName: info.locationName,
      latitude: info.latitude ?? null,
      longitude: info.longitude ?? null,
      tenantId: info.tenantId ?? null,
    });
  },
  registerDeployment() {
    return invokeTauri<string>("register_deployment");
  },
  listDeployments(tenantId: string) {
    return invokeTauri<Array<{
      deployment_id: string;
      deployment_name: string;
      location_name: string;
      latitude: number | null;
      longitude: number | null;
      last_seen: string | null;
      node_count: number | null;
      active_risk_level: string | null;
      online: boolean | null;
    }>>("list_deployments", { tenantId });
  },
  getDeploymentsAggregate(tenantId: string) {
    return invokeTauri<{ total: number; online: number; high_risk: number; avg_risk_score: number }>("get_deployments_aggregate", { tenantId });
  },
  runInsightPipeline(request: {
    sessionId: string;
    deploymentId: string;
    vitalSummary: Record<string, number>;
    poseAnomalies: string[];
    durationSeconds: number;
    csiSnrDb: number;
  }) {
    return invokeTauri<{ status: string; session_name: string; insight_queued: boolean }>(
      "run_insight_pipeline",
      {
        request: {
          session_id: request.sessionId,
          deployment_id: request.deploymentId,
          vital_summary: request.vitalSummary,
          pose_anomalies: request.poseAnomalies,
          duration_seconds: request.durationSeconds,
          csi_snr_db: request.csiSnrDb,
        },
      }
    );
  },
  getSessionInsight(sessionId: string) {
    return invokeTauri<Record<string, unknown> | null>("get_session_insight", { sessionId });
  },
  getAnalyticsTrends() {
    return invokeTauri<{ trends: Array<{ timestamp: string; hr_mean: number; br_mean: number; risk_level: string; risk_score: number }> }>("get_analytics_trends");
  },
  getRiskDistribution() {
    return invokeTauri<{ distribution: { low: number; moderate: number; high: number; critical: number }; total: number }>("get_risk_distribution");
  },
};
