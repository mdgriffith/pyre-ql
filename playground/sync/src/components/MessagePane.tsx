import React, { useEffect, useRef, useMemo } from 'react'
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

type GroupedMessage = 
  | { type: 'request_response'; request: Event; response: Event }
  | { type: 'websocket'; event: Event }

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

  // Group events: pair query_sent with query_response, keep sync_delta separate
  const groupedMessages = useMemo(() => {
    const grouped: GroupedMessage[] = []
    const pendingRequests = new Map<string, Event[]>() // clientId -> array of request events

    for (const event of events) {
      if (event.type === 'query_sent') {
        // Store pending request (FIFO queue per client)
        const key = event.clientId || 'unknown'
        const requests = pendingRequests.get(key) || []
        requests.push(event)
        pendingRequests.set(key, requests)
      } else if (event.type === 'query_response') {
        // Try to pair with pending request (FIFO)
        const key = event.clientId || 'unknown'
        const requests = pendingRequests.get(key)
        
        if (requests && requests.length > 0) {
          // Found a matching request - pair them
          const request = requests.shift()!
          grouped.push({
            type: 'request_response',
            request,
            response: event,
          })
          if (requests.length === 0) {
            pendingRequests.delete(key)
          }
        } else {
          // No matching request, show response alone as websocket message
          grouped.push({ type: 'websocket', event })
        }
      } else {
        // sync_delta or other websocket messages - keep separate
        grouped.push({ type: 'websocket', event })
      }
    }

    // Add any unpaired requests as websocket messages
    for (const requests of pendingRequests.values()) {
      for (const request of requests) {
        grouped.push({ type: 'websocket', event: request })
      }
    }

    return grouped
  }, [events])

  return (
    <div className="message-pane">
      <h2>Message Stream</h2>
      <div className="message-list" ref={scrollRef}>
        {groupedMessages.length === 0 ? (
          <div className="no-messages">No messages yet</div>
        ) : (
          groupedMessages.map((group, index) => {
            if (group.type === 'request_response') {
              // Extract HTTP operation from request data
              const getHttpOperation = () => {
                const requestData = group.request.data
                if (requestData?.url) {
                  // Extract method and path from URL
                  const method = requestData.method || 'GET' // Default to GET if not specified
                  const url = new URL(requestData.url)
                  return `${method} ${url.pathname}${url.search}`
                }
                // Fallback for old format
                if (requestData?.queryId) {
                  const method = requestData.method || 'POST'
                  return `${method} /db/${requestData.queryId}`
                }
                return 'GET /db/:req'
              }

              return (
                <div key={`${group.request.id}-${group.response.id}`} className="message-item message-item-grouped">
                  <div className="message-group-header">
                    <div className="message-group-label">{getHttpOperation()}</div>
                    <div className="message-group-meta">
                      <span className="message-time">
                        {group.request.timestamp.toLocaleTimeString()}
                      </span>
                      {group.request.clientId && (
                        <span className="message-client">
                          {getClientName(group.request.clientId)}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="message-group-content">
                    <div className="message-group-section">
                      <div className="message-section-header" style={{ color: '#007bff' }}>
                        REQUEST
                      </div>
                      <pre className="message-data">
                        {JSON.stringify(group.request.data, null, 2)}
                      </pre>
                    </div>
                    <div className="message-group-section">
                      <div className="message-section-header" style={{ color: '#28a745' }}>
                        RESPONSE
                      </div>
                      <pre className="message-data">
                        {JSON.stringify(group.response.data, null, 2)}
                      </pre>
                    </div>
                  </div>
                </div>
              )
            } else {
              // WebSocket message
              const event = group.event
              const color = event.type === 'sync_delta' ? '#ffc107' : '#666'
              return (
                <div key={event.id} className="message-item">
                  <div className="message-header">
                    <span
                      className="message-type"
                      style={{ color }}
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
              )
            }
          })
        )}
      </div>
    </div>
  )
}
