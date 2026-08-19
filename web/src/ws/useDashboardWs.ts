import { useCallback } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type {
  DashboardData,
  DashboardOverview,
  SiteReseedStats,
  UserInfoAggregate,
} from '../api/types'
import { useWebSocket } from './useWebSocket'

interface DashboardWsUpdate {
  type?: string
  overview?: DashboardOverview | null
  site_stats?: SiteReseedStats[] | null
  user_info?: UserInfoAggregate | null
}

function mergeDashboardData(
  prev: DashboardData | undefined,
  update: DashboardWsUpdate,
): DashboardData | undefined {
  if (!prev) {
    if (!update.overview || !update.site_stats || !update.user_info) {
      return prev
    }
    return {
      overview: update.overview,
      site_stats: update.site_stats,
      trend: [],
      user_info: update.user_info,
    }
  }

  return {
    overview: update.overview ?? prev.overview,
    site_stats: update.site_stats ?? prev.site_stats,
    trend: prev.trend,
    user_info: update.user_info ?? prev.user_info,
  }
}

export function useDashboardWs() {
  const queryClient = useQueryClient()

  const onMessage = useCallback((event: MessageEvent) => {
    try {
      const update = JSON.parse(event.data) as DashboardWsUpdate
      if (update.type && update.type !== 'dashboard_update') return

      queryClient.setQueriesData<DashboardData>(
        { queryKey: ['dashboard'] },
        (prev) => mergeDashboardData(prev, update),
      )
    } catch {
      // ignore malformed messages
    }
  }, [queryClient])

  const { connected, reconnect } = useWebSocket({
    path: '/ws/dashboard',
    onMessage,
  })

  return { connected, reconnect }
}
