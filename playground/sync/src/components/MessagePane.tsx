import React, { useEffect, useRef, useMemo, useState } from 'react'
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
  | { type: 'sse'; event: Event }

export default function MessagePane({ events, clients }: MessagePaneProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const [expandedMessages, setExpandedMessages] = useState<Set<string>>(new Set())

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

  const toggleMessage = (messageId: string) => {
    setExpandedMessages((prev) => {
      const next = new Set(prev)
      if (next.has(messageId)) {
        next.delete(messageId)
      } else {
        next.add(messageId)
      }
      return next
    })
  }

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
    } catch (err) {
      console.error('Failed to copy:', err)
    }
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
          // No matching request, show response alone as SSE message
          grouped.push({ type: 'sse', event })
        }
      } else {
        // sync_delta or other SSE messages - keep separate
        grouped.push({ type: 'sse', event })
      }
    }

    // Add any unpaired requests as SSE messages
    for (const requests of pendingRequests.values()) {
      for (const request of requests) {
        grouped.push({ type: 'sse', event: request })
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

              const getFullUrl = () => {
                const requestData = group.request.data
                if (requestData?.url) {
                  return requestData.url
                }
                if (requestData?.queryId) {
                  const sessionId = requestData.sessionId
                  return sessionId 
                    ? `http://localhost:3000/db/${requestData.queryId}?sessionId=${sessionId}`
                    : `http://localhost:3000/db/${requestData.queryId}`
                }
                return ''
              }

              const messageId = `${group.request.id}-${group.response.id}`
              const isExpanded = expandedMessages.has(messageId)
              const fullUrl = getFullUrl()

              return (
                <div key={messageId} className="message-item message-item-grouped">
                  <div 
                    className={`message-group-header ${isExpanded ? 'expanded' : ''}`}
                    onClick={() => toggleMessage(messageId)}
                    style={{ cursor: 'pointer' }}
                  >
                    <div className="message-group-header-left">
                      <svg
                        className={`message-caret ${isExpanded ? 'expanded' : ''}`}
                        width="12"
                        height="12"
                        viewBox="0 0 12 12"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <path
                          d="M4.5 3L7.5 6L4.5 9"
                          stroke="currentColor"
                          strokeWidth="1.5"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </svg>
                      {fullUrl && (
                        <button
                          className="message-clipboard-btn"
                          onClick={(e) => {
                            e.stopPropagation()
                            copyToClipboard(fullUrl)
                          }}
                          title={`Copy URL to clipboard: ${fullUrl}`}
                        >
                          <svg
                            width="14"
                            height="14"
                            viewBox="0 0 14 14"
                            fill="none"
                            xmlns="http://www.w3.org/2000/svg"
                          >
                            <path
                              d="M9.5 1.5H4.5C3.67157 1.5 3 2.17157 3 3V9.5C3 10.3284 3.67157 11 4.5 11H9.5C10.3284 11 11 10.3284 11 9.5V3C11 2.17157 10.3284 1.5 9.5 1.5Z"
                              stroke="currentColor"
                              strokeWidth="1.2"
                              strokeLinecap="round"
                              strokeLinejoin="round"
                            />
                            <path
                              d="M6.5 1.5V3.5C6.5 4.05228 6.94772 4.5 7.5 4.5H9.5"
                              stroke="currentColor"
                              strokeWidth="1.2"
                              strokeLinecap="round"
                              strokeLinejoin="round"
                            />
                          </svg>
                        </button>
                      )}
                      <div className="message-group-label">{getHttpOperation()}</div>
                    </div>
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
                  {isExpanded && (
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
                  )}
                </div>
              )
            } else {
              // SSE message
              const event = group.event
              const color = event.type === 'sync_delta' ? '#ffc107' : '#666'
              const messageId = event.id
              const isExpanded = expandedMessages.has(messageId)

              return (
                <div key={messageId} className="message-item">
                  <div 
                    className="message-header"
                    onClick={() => toggleMessage(messageId)}
                    style={{ cursor: 'pointer' }}
                  >
                    <div className="message-header-left">
                      <svg
                        className={`message-caret ${isExpanded ? 'expanded' : ''}`}
                        width="12"
                        height="12"
                        viewBox="0 0 12 12"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <path
                          d="M4.5 3L7.5 6L4.5 9"
                          stroke="currentColor"
                          strokeWidth="1.5"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </svg>
                      <span
                        className="message-type"
                        style={{ color }}
                      >
                        {event.type}
                      </span>
                    </div>
                    <div className="message-header-right">
                      <span className="message-time">
                        {event.timestamp.toLocaleTimeString()}
                      </span>
                      {event.clientId && (
                        <span className="message-client">
                          {getClientName(event.clientId)}
                        </span>
                      )}
                    </div>
                  </div>
                  {isExpanded && (
                    <pre className="message-data">
                      {JSON.stringify(event.data, null, 2)}
                    </pre>
                  )}
                </div>
              )
            }
          })
        )}
      </div>
    </div>
  )
}
