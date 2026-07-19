/**
 * Cross-feature integration constants and utilities.
 * Ties together features from all 8 production branches.
 */

// feature/cloud-tier-gating: plan tiers
export { usePlanStore } from './plan-store'
// Named export (upgrade-prompt uses named export, not default)
export { UpgradePrompt } from '../components/upgrade-prompt'

// feature/langfuse-tracing: trace URL builder
export function langfuseTraceUrl(traceId: string): string {
  const host = 'https://cloud.langfuse.com'
  return `${host}/trace/${traceId}`
}

// feature/cloud-upload-consent: consent key
export const CLOUD_CONSENT_KEY = 'ruview-cloud-consent-granted'
export function hasCloudConsent(): boolean {
  return localStorage.getItem(CLOUD_CONSENT_KEY) === 'true'
}

// feature/latentcsi-image-gen: vision API URL
export function visionApiUrl(): string {
  return (window as unknown as Record<string, unknown>)['__RUVIEW_CLOUD_ENDPOINT__'] as string
    || 'http://localhost:8001'
}

// feature/huggingface-model-hub: local model check
export async function hasLocalModel(repoId: string): Promise<boolean> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const models = await invoke<Array<{ repo_id: string }>>('list_local_models')
    return models.some(m => m.repo_id === repoId)
  } catch { return false }
}

// feature/security-hardening: auth header for API calls
export function bearerHeader(token: string): HeadersInit {
  return token ? { 'Authorization': `Bearer ${token}` } : {}
}
