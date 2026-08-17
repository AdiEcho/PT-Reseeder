import { useEffect, useRef, useState, useCallback } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type {
  DashboardData,
  DashboardOverview,
  SiteReseedStats,
  UserInfoAggregate,
} from '../api/types'

interface DashboardWsUpdate {
  type?: string
  overview?: DashboardOverview | null
  site_stats?: SiteReseedStats[] | null
  user_info?: UserInfoAggregate | null
}

function getWsUrl(path: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}${path}`
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
  const [connected, setConnected] = useState(false)
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const backoffRef = useRef(1000)
  const unmountedRef = useRef(false)

  const connect = useCallback(() => {
    if (unmountedRef.current) return

    const ws = new WebSocket(getWsUrl('/ws/dashboard'))
    wsRef.current = ws

    ws.onopen = () => {
      setConnected(true)
      backoffRef.current = 1000
    }

    ws.onmessage = (event) => {
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
    }

    ws.onclose = () => {
      setConnected(false)
      if (!unmountedRef.current) {
        reconnectTimer.current = setTimeout(() => {
          backoffRef.current = Math.min(backoffRef.current * 2, 30000)
          connect()
        }, backoffRef.current)
      }
    }

    ws.onerror = () => {
      ws.close()
    }
  }, [queryClient])

  useEffect(() => {
    unmountedRef.current = false
    connect()

    return () => {
      unmountedRef.current = true
      if (reconnectTimer.current) {
        clearTimeout(reconnectTimer.current)
      }
      wsRef.current?.close()
    }
  }, [connect])

  return { connected }
}
