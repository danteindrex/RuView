import { invoke } from '@tauri-apps/api/core'

interface ConsentGateProps {
  onConsent: () => void
  onDecline: () => void
}

export function ConsentGate({ onConsent, onDecline }: ConsentGateProps) {
  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-900 rounded-lg p-6 max-w-md mx-4 shadow-xl">
        <h2 className="text-lg font-semibold mb-3">Cloud Data Consent</h2>
        <p className="text-sm text-gray-600 dark:text-gray-300 mb-3">
          This app will transmit AES-256 encrypted sensing data to Wave servers:
        </p>
        <ul className="text-sm text-gray-600 dark:text-gray-300 mb-4 list-disc pl-5 space-y-1">
          <li>Vital signs (heart rate, breathing rate)</li>
          <li>Pose anomaly events (fall detection, gait)</li>
          <li>Session metadata (duration, device ID)</li>
        </ul>
        <p className="text-xs text-gray-400 mb-6">You can withdraw consent anytime in Settings &rarr; Cloud Sync.</p>
        <div className="flex gap-3">
          <button
            onClick={async () => { await invoke('set_consent', { granted: true }); onConsent() }}
            className="flex-1 bg-blue-600 text-white py-2 rounded-md text-sm font-medium hover:bg-blue-700"
          >I Agree</button>
          <button
            onClick={() => { invoke('set_consent', { granted: false }); onDecline() }}
            className="flex-1 border border-gray-300 py-2 rounded-md text-sm hover:bg-gray-50"
          >Decline</button>
        </div>
      </div>
    </div>
  )
}
