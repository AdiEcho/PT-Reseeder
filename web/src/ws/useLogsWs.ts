import { useState, useCallback } from 'react'
import { useWebSocket } from './useWebSocket'

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

export function useLogsWs() {
  const [lines, setLines] = useState<ParsedLogLine[]>([])

  const clear = useCallback(() => {
    setLines([])
  }, [])

  const onMessage = useCallback((event: MessageEvent) => {
    const parsed = parseLine(event.data as string)
    setLines((prev) => {
      const next = [...prev, parsed]
      return next.length > MAX_BUFFER ? next.slice(next.length - MAX_BUFFER) : next
    })
  }, [])

  const { connected, reconnect } = useWebSocket({
    path: '/ws/logs',
    onMessage,
  })

  return { lines, connected, clear, reconnect }
}
