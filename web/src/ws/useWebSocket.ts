import { useEffect, useRef, useState, useCallback } from 'react'

export type WsConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting'

export interface UseWebSocketOptions {
  /** WebSocket path (e.g. '/ws/logs') */
  path: string
  /** Called on each incoming message */
  onMessage: (event: MessageEvent) => void
  /** Whether the connection should be active (default: true) */
  enabled?: boolean
}

export function getWsUrl(path: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}${path}`
}

/**
 * Shared WebSocket hook with exponential backoff + jitter and
 * page-visibility handling.
 */
export function useWebSocket(options: UseWebSocketOptions) {
  const { path, onMessage, enabled = true } = options

  const [state, setState] = useState<WsConnectionState>('disconnected')
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const backoffRef = useRef(1000)
  const unmountedRef = useRef(false)
  const onMessageRef = useRef(onMessage)
  onMessageRef.current = onMessage

  const clearReconnectTimer = useCallback(() => {
    if (reconnectTimer.current) {
      clearTimeout(reconnectTimer.current)
      reconnectTimer.current = null
    }
  }, [])

  const connect = useCallback(() => {
    if (unmountedRef.current || !enabled) return
    if (document.hidden) return // don't connect while page is hidden

    setState('connecting')
    const ws = new WebSocket(getWsUrl(path))
    wsRef.current = ws

    ws.onopen = () => {
      setState('connected')
      backoffRef.current = 1000
    }

    ws.onmessage = (event) => {
      onMessageRef.current(event)
    }

    ws.onclose = () => {
      setState('disconnected')
      if (!unmountedRef.current && enabled && !document.hidden) {
        scheduleReconnect()
      }
    }

    ws.onerror = () => {
      ws.close()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, enabled])

  const scheduleReconnect = useCallback(() => {
    if (unmountedRef.current) return
    setState('reconnecting')
    // Exponential backoff with jitter (random 0-1s added)
    const jitter = Math.random() * 1000
    const delay = backoffRef.current + jitter
    reconnectTimer.current = setTimeout(() => {
      backoffRef.current = Math.min(backoffRef.current * 2, 30000)
      connect()
    }, delay)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connect])

  const reconnect = useCallback(() => {
    clearReconnectTimer()
    wsRef.current?.close()
    backoffRef.current = 1000
    connect()
  }, [clearReconnectTimer, connect])

  // Page visibility handler: pause reconnect when hidden, resume when visible
  useEffect(() => {
    const handleVisibility = () => {
      if (document.hidden) {
        // Pause: cancel pending reconnect
        clearReconnectTimer()
      } else {
        // Resume: reconnect if disconnected
        if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
          connect()
        }
      }
    }

    document.addEventListener('visibilitychange', handleVisibility)
    return () => {
      document.removeEventListener('visibilitychange', handleVisibility)
    }
  }, [clearReconnectTimer, connect])

  // Main connection lifecycle
  useEffect(() => {
    unmountedRef.current = false
    if (enabled) {
      connect()
    }

    return () => {
      unmountedRef.current = true
      clearReconnectTimer()
      wsRef.current?.close()
    }
  }, [connect, enabled, clearReconnectTimer])

  return { state, connected: state === 'connected', reconnect }
}
