import { http } from '../lib/http'

export interface Page<T> { items: T[]; page: number; page_size: number; total: number }
export interface User { id:string; username:string; display_name:string; role:string; is_admin:boolean; is_active?:boolean; version?:number }
export interface Resource { id:string; version?:number; [key:string]:unknown }

export async function getPage<T>(endpoint:string, q='', page=1, params:Record<string,unknown>={}) {
  return (await http.get<Page<T>>(endpoint,{params:{q,page,page_size:20,...params}})).data
}
export async function createResource<T>(endpoint:string,data:unknown) { return (await http.post<T>(endpoint,data)).data }
export async function updateResource<T>(endpoint:string,id:string,data:unknown) { return (await http.put<T>(`${endpoint}/${id}`,data)).data }
export async function deleteResource(endpoint:string,id:string) { await http.delete(`${endpoint}/${id}`) }

export const authApi = {
  me: async () => (await http.get<User>('/auth/me')).data,
  login: async (username:string,password:string) => (await http.post<User>('/auth/login',{username,password})).data,
  logout: async () => { await http.post('/auth/logout') },
}
