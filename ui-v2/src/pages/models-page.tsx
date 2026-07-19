import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

interface HfModel { model_id: string; downloads?: number; tags?: string[]; siblings?: Array<{rfilename: string; size?: number}> }
interface LocalModel { repo_id: string; filename: string; local_path: string; size_bytes: number; downloaded_at: string }

export function ModelsPage() {
  const [tab, setTab] = useState<'search'|'local'>('search')
  const [query, setQuery] = useState('wifi-csi')
  const [results, setResults] = useState<HfModel[]>([])
  const [local, setLocal] = useState<LocalModel[]>([])
  const [progress, setProgress] = useState<Record<string, number>>({})
  const [searching, setSearching] = useState(false)
  const [error, setError] = useState<string|null>(null)

  useEffect(() => {
    const unlisten = listen<{repo_id: string; percent: number}>('hf://download/progress', ev => {
      setProgress(p => ({ ...p, [ev.payload.repo_id]: ev.payload.percent }))
    })
    handleSearch()
    loadLocal()
    return () => { unlisten.then(f => f()) }
  }, [])

  const handleSearch = async () => {
    setSearching(true); setError(null)
    try { setResults(await invoke<HfModel[]>('search_hf_models', { query })) }
    catch (e) { setError(String(e)) }
    finally { setSearching(false) }
  }

  const loadLocal = async () => {
    try { setLocal(await invoke<LocalModel[]>('list_local_models')) } catch {}
  }

  const download = async (model: HfModel, file: string) => {
    setProgress(p => ({ ...p, [model.model_id]: 0 }))
    try {
      await invoke('download_hf_model', { repoId: model.model_id, filename: file })
      setProgress(p => { const n = { ...p }; delete n[model.model_id]; return n })
      loadLocal()
    } catch (e) {
      setError(String(e))
      setProgress(p => { const n = { ...p }; delete n[model.model_id]; return n })
    }
  }

  const fmt = (b?: number) => b ? `${(b/1024/1024).toFixed(1)} MB` : '?'

  return (
    <div className="space-y-6">
      <div className="flex gap-2 border-b border-border/60">
        {(['search','local'] as const).map(t => (
          <button key={t} onClick={() => { setTab(t); if(t==='local') loadLocal() }}
            className={`px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors ${tab===t ? 'border-primary text-primary' : 'border-transparent text-muted-foreground hover:text-foreground'}`}>
            {t === 'search' ? 'HuggingFace Search' : `My Models (${local.length})`}
          </button>
        ))}
      </div>

      {tab === 'search' && (
        <div className="space-y-4">
          <div className="flex gap-2">
            <input value={query} onChange={e => setQuery(e.target.value)}
              onKeyDown={e => e.key==='Enter' && handleSearch()}
              placeholder="wifi-csi, rf-sensing, csi-pose..."
              className="flex-1 border border-border/60 rounded-lg bg-background px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-primary/60" />
            <button onClick={handleSearch} disabled={searching}
              className="bg-primary text-primary-foreground px-4 py-2 rounded-lg text-sm font-medium hover:bg-primary/90 disabled:opacity-50 transition-colors">
              {searching ? '...' : 'Search'}
            </button>
          </div>
          {error && <p className="text-destructive text-sm">{error}</p>}
          <div className="space-y-3">
            {results.map(m => {
              const onnxFiles = m.siblings?.filter(s => s.rfilename.endsWith('.onnx')) ?? []
              const pct = progress[m.model_id]
              return (
                <div key={m.model_id} className="border border-border/60 rounded-lg p-4 space-y-2 bg-card/40">
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="font-medium text-sm">{m.model_id}</p>
                      <p className="text-xs text-muted-foreground">{m.downloads?.toLocaleString()} downloads</p>
                    </div>
                    <div className="flex gap-1 flex-wrap justify-end">
                      {m.tags?.slice(0,3).map(t => <span key={t} className="text-xs bg-secondary/50 px-2 py-0.5 rounded font-medium">{t}</span>)}
                    </div>
                  </div>
                  {onnxFiles.length > 0 ? onnxFiles.map(f => (
                    <div key={f.rfilename} className="flex items-center justify-between text-xs">
                      <span className="text-muted-foreground">{f.rfilename} ({fmt(f.size)})</span>
                      {pct !== undefined ? (
                        <div className="flex items-center gap-2">
                          <div className="w-24 h-1.5 bg-secondary rounded-full overflow-hidden">
                            <div className="h-full bg-primary rounded-full transition-all" style={{width:`${pct}%`}} />
                          </div>
                          <span className="text-muted-foreground">{pct.toFixed(0)}%</span>
                        </div>
                      ) : (
                        <button onClick={() => download(m, f.rfilename)}
                          className="bg-primary text-primary-foreground px-3 py-1 rounded font-medium hover:bg-primary/90 transition-colors">
                          Download
                        </button>
                      )}
                    </div>
                  )) : <p className="text-xs text-muted-foreground">No ONNX files available</p>}
                </div>
              )
            })}
            {!searching && results.length === 0 && (
              <p className="text-muted-foreground text-sm text-center py-8">No results. Try "wifi-csi" or "rf-sensing".</p>
            )}
          </div>
        </div>
      )}

      {tab === 'local' && (
        <div className="space-y-3">
          {local.length === 0
            ? <p className="text-muted-foreground text-sm text-center py-8">No models downloaded yet.</p>
            : local.map(m => (
              <div key={m.repo_id} className="border border-border/60 rounded-lg p-4 flex items-center justify-between bg-card/40">
                <div>
                  <p className="font-medium text-sm">{m.repo_id}</p>
                  <p className="text-xs text-muted-foreground">{m.filename} · {fmt(m.size_bytes)}</p>
                  <p className="text-xs text-muted-foreground">Downloaded {new Date(m.downloaded_at).toLocaleDateString()}</p>
                </div>
                <button onClick={() => invoke('delete_local_model', { repoId: m.repo_id }).then(() => loadLocal())}
                  className="text-destructive text-xs border border-destructive/30 px-3 py-1 rounded font-medium hover:bg-destructive/10 transition-colors">
                  Delete
                </button>
              </div>
            ))}
        </div>
      )}
    </div>
  )
}
