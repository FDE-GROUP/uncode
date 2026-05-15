import { useEffect, useRef, useState, useCallback } from 'react'

export interface LiveEvent {
  type: string
  [key: string]: unknown
}

export function useEvents(url = 'ws://127.0.0.1:3000/ws/events') {
  const [events, setEvents] = useState<LiveEvent[]>([])
  const [connected, setConnected] = useState(false)
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectRef = useRef<ReturnType<typeof setTimeout>>(undefined)

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return

    const ws = new WebSocket(url)

    ws.onopen = () => {
      setConnected(true)
    }

    ws.onclose = () => {
      setConnected(false)
      wsRef.current = null
      // Auto-reconnect after 3s
      reconnectRef.current = setTimeout(connect, 3000)
    }

    ws.onmessage = (e) => {
      try {
        const event: LiveEvent = JSON.parse(e.data)
        setEvents((prev) => [...prev.slice(-99), event])
      } catch {
        // ignore malformed messages
      }
    }

    ws.onerror = () => {
      ws.close()
    }

    wsRef.current = ws
  }, [url])

  useEffect(() => {
    connect()
    return () => {
      clearTimeout(reconnectRef.current)
      wsRef.current?.close()
    }
  }, [connect])

  const clear = useCallback(() => setEvents([]), [])

  return { events, connected, clear }
}
