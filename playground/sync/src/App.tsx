import React, { useState, useEffect, useCallback } from 'react'
import ClientList from './components/ClientList'
import QueryForm from './components/QueryForm'
import MessagePane from './components/MessagePane'
import { discoverQueries, QueryMetadata } from './queryDiscovery'
import './App.css'

interface Client {
  id: string
  name: string
  ws: WebSocket | null
  sessionId: string | null
  connected: boolean
}

interface Event {
  id: string
  type: 'query_sent' | 'query_response' | 'sync_delta'
  timestamp: Date
  data: any
  clientId?: string
}

function App() {
  const [clients, setClients] = useState<Client[]>([
    {
      id: '1',
      name: 'Client 1',
      ws: null,
      sessionId: null,
      connected: false,
    },
  ])
  const [selectedClientId, setSelectedClientId] = useState<string>('1')
  const [events, setEvents] = useState<Event[]>([])
  const [queries, setQueries] = useState<QueryMetadata[]>([])

  // Discover queries on mount
  useEffect(() => {
    const loadQueries = async () => {
      try {
        const discovered = await discoverQueries()
        setQueries(discovered)
      } catch (error) {
        console.error('Failed to discover queries:', error)
      }
    }
    loadQueries()
  }, [])

  // Connect WebSocket for initial client
  useEffect(() => {
    connectClient('1')
  }, [])

  const connectClient = useCallback((clientId: string) => {
    const client = clients.find((c) => c.id === clientId)
    if (!client || client.connected) return

    const ws = new WebSocket('ws://localhost:3000/sync')

    ws.onopen = () => {
      setClients((prev) =>
        prev.map((c) =>
          c.id === clientId ? { ...c, ws, connected: true } : c
        )
      )
      addEvent({
        type: 'query_sent',
        data: { message: 'Connecting to server...' },
        clientId,
      })
    }

    ws.onmessage = (event: MessageEvent) => {
      try {
        const message = JSON.parse(event.data)

        if (message.type === 'connected') {
          setClients((prev) =>
            prev.map((c) =>
              c.id === clientId
                ? { ...c, sessionId: message.sessionId, connected: true }
                : c
            )
          )
          addEvent({
            type: 'query_response',
            data: { message: 'Connected', sessionId: message.sessionId },
            clientId,
          })
        } else if (message.type === 'delta') {
          addEvent({
            type: 'sync_delta',
            data: message.data,
            clientId,
          })
        }
      } catch (error) {
        console.error('Failed to parse WebSocket message:', error)
      }
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
      addEvent({
        type: 'query_response',
        data: { error: 'WebSocket connection error' },
        clientId,
      })
    }

    ws.onclose = () => {
      setClients((prev) =>
        prev.map((c) =>
          c.id === clientId ? { ...c, connected: false, ws: null } : c
        )
      )
    }
  }, [clients])

  const addEvent = useCallback((event: Omit<Event, 'id' | 'timestamp'>) => {
    setEvents((prev) => [
      ...prev,
      {
        ...event,
        id: `${Date.now()}-${Math.random()}`,
        timestamp: new Date(),
      },
    ])
  }, [])

  const addNewClient = useCallback(() => {
    const newId = `${clients.length + 1}`
    const newClient: Client = {
      id: newId,
      name: `Client ${newId}`,
      ws: null,
      sessionId: null,
      connected: false,
    }
    setClients((prev) => [...prev, newClient])
    connectClient(newId)
  }, [clients, connectClient])

  const submitQuery = useCallback(
    async (queryId: string, params: Record<string, any>) => {
      if (!selectedClientId) return

      const client = clients.find((c) => c.id === selectedClientId)
      if (!client) return

      // Log query sent
      addEvent({
        type: 'query_sent',
        data: { queryId, params },
        clientId: selectedClientId,
      })

      try {
        const response = await fetch(`http://localhost:3000/db/${queryId}`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(params),
        })

        const result = await response.json()

        addEvent({
          type: 'query_response',
          data: result,
          clientId: selectedClientId,
        })
      } catch (error: any) {
        addEvent({
          type: 'query_response',
          data: { error: error.message },
          clientId: selectedClientId,
        })
      }
    },
    [selectedClientId, clients, addEvent]
  )

  return (
    <div className="app">
      <header className="app-header">
        <h1>Pyre Sync Playground</h1>
        <button onClick={addNewClient} className="add-client-btn">
          + Add Client
        </button>
      </header>
      <div className="app-content">
        <div className="left-panel">
          <ClientList
            clients={clients}
            selectedClientId={selectedClientId}
            onSelectClient={setSelectedClientId}
          />
          <QueryForm
            queries={queries}
            onSubmit={submitQuery}
            selectedClient={clients.find((c) => c.id === selectedClientId)}
          />
        </div>
        <div className="right-panel">
          <MessagePane events={events} clients={clients} />
        </div>
      </div>
    </div>
  )
}

export default App
