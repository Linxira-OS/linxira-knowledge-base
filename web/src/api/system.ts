import { api } from './client'
import type { FeaturesResponse } from './types'

export function getFeatures(): Promise<FeaturesResponse> {
  return api<FeaturesResponse>('/api/v1/features')
}

export interface HealthResponse {
  status: string
  profile?: string
  backend?: string
  fts?: boolean
  seed?: unknown
}

export function getHealth(): Promise<HealthResponse> {
  return api<HealthResponse>('/health')
}
