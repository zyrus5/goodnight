import { http } from '../lib/http'

export interface HealthResponse {
  service: 'ok'
  database: 'ok' | 'unavailable'
}

export async function getHealth() {
  const { data } = await http.get<HealthResponse>('/health')
  return data
}
