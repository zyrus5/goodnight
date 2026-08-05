import { create } from 'zustand'

interface AppState {
  visits: number
  incrementVisits: () => void
}

export const useAppStore = create<AppState>((set) => ({
  visits: 0,
  incrementVisits: () => set((state) => ({ visits: state.visits + 1 })),
}))
