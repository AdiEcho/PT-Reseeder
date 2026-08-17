import { useQuery } from '@tanstack/react-query'
import { api } from '../client'
import type { DashboardData } from '../types'

export function useDashboard(days?: number) {
  const qs = days != null ? `?days=${days}` : ''
  return useQuery({
    queryKey: ['dashboard', days],
    queryFn: () => api.get<DashboardData>(`/api/dashboard${qs}`),
  })
}
