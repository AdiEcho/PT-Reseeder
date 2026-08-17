import { useEffect, useRef, useState, useCallback } from 'react'

export interface ParsedLogLine {
  timestamp: string
  level: string
  target: string
  message: string
}

const MAX_BUFFER = 500

function parseLine(raw: string): ParsedLogLine {
  // Format: "TIMESTAMP LEVEL TARGET MESSAGE" — split on first 3 spaces
  const parts = raw.split(' ')
  if (parts.length >= 4) {
    return {
      timestamp: parts[0],
      level: parts[1],
      target: parts[2],
      message: parts.slice(3).join(' '),
    }
  }
  // fallback: unparseable line
  return { timestamp: '', level: 'INFO', target: '', message: raw }
}

function getWsUrl(path: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${window.location.host}${path}`
}

export function useLogsWs() {
  const [lines, setLines] = useState<ParsedLogLine[]>([])
  const [connected, setConnected] = useState(false)
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const backoffRef = useRef(1000)
  const unmountedRef = useRef(false)

  const clear = useCallback(() => {
    setLines([])
  }, [])

  const connect = useCallback(() => {
    if (unmountedRef.current) return

    const ws = new WebSocket(getWsUrl('/ws/logs'))
    wsRef.current = ws

    ws.onopen = () => {
      setConnected(true)
      backoffRef.current = 1000
    }

    ws.onmessage = (event) => {
      const parsed = parseLine(event.data as string)
      setLines((prev) => {
        const next = [...prev, parsed]
        return next.length > MAX_BUFFER ? next.slice(next.length - MAX_BUFFER) : next
      })
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
  }, [])

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

  return { lines, connected, clear }
}
