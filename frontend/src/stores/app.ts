import { create } from 'zustand'
import { authApi, type User } from '../services/api'

interface AppState {
  user: User | null
  ready: boolean
  loadUser: () => Promise<void>
  login: (username:string,password:string) => Promise<void>
  logout: () => Promise<void>
}
export const useAppStore=create<AppState>((set)=>({
  user:null,ready:false,
  loadUser:async()=>{try{set({user:await authApi.me(),ready:true})}catch{set({user:null,ready:true})}},
  login:async(username,password)=>set({user:await authApi.login(username,password),ready:true}),
  logout:async()=>{await authApi.logout();set({user:null})},
}))
