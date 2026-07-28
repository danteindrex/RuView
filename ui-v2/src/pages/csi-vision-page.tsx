import { useState, useCallback } from 'react'
import { useSensingStore } from '@/lib/sensing-store'

interface VisionState {
  currentImage: string | null
  isGenerating: boolean
  prompt: string
  strength: number
  history: string[]
  error: string | null
}

export default function CsiVisionPage() {
  const [state, setState] = useState<VisionState>({
    currentImage: null, isGenerating: false,
    prompt: '', strength: 0.6, history: [], error: null,
  })

  // TODO: replace with usePlanStore().isCloud() from feature/cloud-tier-gating
  const isCloudPlan = false

  // Real CSI amplitudes from a live sensing node. No fabricated data — when a
  // node is streaming, csiAmplitudes is a real vector; otherwise it's null and
  // generation is disabled.
  const latestUpdate = useSensingStore((s) => s.latestUpdate)
  const csiAmplitudes =
    latestUpdate?.nodes?.[0]?.amplitude && latestUpdate.nodes[0].amplitude.length > 0
      ? latestUpdate.nodes[0].amplitude
      : null
  const hasCsi = csiAmplitudes !== null

  const generate = useCallback(async () => {
    if (!isCloudPlan) return
    if (!csiAmplitudes) {
      setState(s => ({ ...s, error: 'No CSI data available from a live node.' }))
      return
    }
    setState(s => ({ ...s, isGenerating: true, error: null }))
    try {
      const resp = await fetch('http://localhost:8001/vision/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ csi_amplitudes: csiAmplitudes, prompt: state.prompt, strength: state.strength }),
      })
      if (!resp.ok) throw new Error(`Vision API ${resp.status}`)
      const data = await resp.json()
      const src = `data:image/png;base64,${data.image_base64}`
      setState(s => ({ ...s, currentImage: src, isGenerating: false, history: [src, ...s.history].slice(0, 6) }))
    } catch (e) {
      setState(s => ({ ...s, isGenerating: false, error: String(e) }))
    }
  }, [state.prompt, state.strength, isCloudPlan, csiAmplitudes])

  if (!isCloudPlan) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <div className="text-5xl mb-4">🔒</div>
        <h2 className="text-xl font-semibold mb-2">CSI Vision — Cloud Plan Required</h2>
        <p className="text-gray-500 mb-6 max-w-sm">
          Real-time environment image generation from WiFi CSI uses Stable Diffusion v1.5
          running on cloud infrastructure.
        </p>
        <a href="mailto:sales@wave.io"
           className="bg-blue-600 text-white px-6 py-2 rounded-md hover:bg-blue-700">
          Upgrade to Cloud Plan
        </a>
      </div>
    )
  }

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-bold">CSI Vision</h1>
        <p className="text-sm text-gray-500 mt-1">
          Generate environment images from WiFi CSI using Stable Diffusion v1.5 (LatentCSI)
        </p>
      </div>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="border rounded-lg overflow-hidden bg-gray-50 dark:bg-gray-900 aspect-square flex items-center justify-center">
          {state.isGenerating ? (
            <div className="text-center text-gray-400">
              <div className="text-3xl mb-2 animate-spin">⟳</div>
              <p className="text-sm">Generating from CSI...</p>
            </div>
          ) : state.currentImage ? (
            <img src={state.currentImage} alt="CSI-generated" className="w-full h-full object-cover" />
          ) : (
            <div className="text-center text-gray-400">
              <p className="text-4xl mb-2">📡</p>
              <p className="text-sm">No image generated</p>
            </div>
          )}
        </div>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Text Prompt (optional)</label>
            <input type="text" value={state.prompt}
              onChange={e => setState(s => ({ ...s, prompt: e.target.value }))}
              placeholder="e.g. a person in a hospital room, realistic"
              className="w-full border rounded px-3 py-2 text-sm" />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">
              Diffusion Strength: {state.strength.toFixed(1)}
            </label>
            <input type="range" min="0.3" max="0.9" step="0.1"
              value={state.strength}
              onChange={e => setState(s => ({ ...s, strength: parseFloat(e.target.value) }))}
              className="w-full" />
            <p className="text-xs text-gray-400 mt-1">Lower = closer to CSI · Higher = more creative</p>
          </div>
          <button onClick={generate} disabled={state.isGenerating || !hasCsi}
            className="w-full bg-blue-600 text-white py-2 rounded-md hover:bg-blue-700 disabled:opacity-50">
            {state.isGenerating ? 'Generating...' : 'Generate from Live CSI'}
          </button>
          {!hasCsi && <p className="text-amber-500 text-sm">No CSI data available from a live node.</p>}
          {state.error && <p className="text-red-500 text-sm">{state.error}</p>}
        </div>
      </div>
      {state.history.length > 1 && (
        <div>
          <h3 className="text-sm font-medium mb-2">Recent Images</h3>
          <div className="grid grid-cols-6 gap-2">
            {state.history.map((img, i) => (
              <img key={i} src={img} className="aspect-square object-cover rounded border cursor-pointer hover:opacity-80"
                onClick={() => setState(s => ({ ...s, currentImage: img }))} />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
