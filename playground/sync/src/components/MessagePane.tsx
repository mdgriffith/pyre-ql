import React, { useEffect, useRef } from 'react'
import './MessagePane.css'

interface Client {
  id: string
  name: string
}

interface Event {
  id: string
  type: 'query_sent' | 'query_response' | 'sync_delta'
  timestamp: Date
  data: any
  clientId?: string
}

interface MessagePaneProps {
  events: Event[]
  clients: Client[]
}

export default function MessagePane({ events, clients }: MessagePaneProps) {
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [events])

  const getClientName = (clientId?: string) => {
    if (!clientId) return 'Unknown'
    const client = clients.find((c) => c.id === clientId)
    return client?.name || clientId
  }

  const getEventColor = (type: Event['type']) => {
    switch (type) {
      case 'query_sent':
        return '#007bff'
      case 'query_response':
        return '#28a745'
      case 'sync_delta':
        return '#ffc107'
      default:
        return '#333'
    }
  }

  return (
    <div className="message-pane">
      <h2>Message Stream</h2>
      <div className="message-list" ref={scrollRef}>
        {events.length === 0 ? (
          <div className="no-messages">No messages yet</div>
        ) : (
          events.map((event) => (
            <div key={event.id} className="message-item">
              <div className="message-header">
                <span
                  className="message-type"
                  style={{ color: getEventColor(event.type) }}
                >
                  {event.type}
                </span>
                <span className="message-time">
                  {event.timestamp.toLocaleTimeString()}
                </span>
                {event.clientId && (
                  <span className="message-client">
                    {getClientName(event.clientId)}
                  </span>
                )}
              </div>
              <pre className="message-data">
                {JSON.stringify(event.data, null, 2)}
              </pre>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
