import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'

type Tier = 'local' | 'cloud' | 'enterprise'

interface PlanStore {
  tier: Tier
  isCloud: () => boolean
  isEnterprise: () => boolean
  load: () => Promise<void>
}

export const usePlanStore = create<PlanStore>((set, get) => ({
  tier: 'local',
  isCloud: () => ['cloud', 'enterprise'].includes(get().tier),
  isEnterprise: () => get().tier === 'enterprise',
  load: async () => {
    try {
      const tier = await invoke<string>('get_plan_tier')
      set({ tier: tier as Tier })
    } catch {
      set({ tier: 'local' })
    }
  },
}))
