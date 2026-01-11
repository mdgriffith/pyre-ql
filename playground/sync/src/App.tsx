import React, { useState, useEffect, useCallback, useRef } from 'react'
import ClientList from './components/ClientList'
import QueryForm from './components/QueryForm'
import MessagePane from './components/MessagePane'
import Tableau from './components/Tableau'
import { discoverQueries, QueryMetadata } from './queryDiscovery'
import './App.css'

interface SyncCursor {
  tables: Record<string, {
    last_seen_updated_at: number | null
    permission_hash: string
  }>
}

interface ClientData {
  tables: Record<string, any[]>
}

interface Client {
  id: string
  name: string
  ws: WebSocket | null
  sessionId: string | null
  connected: boolean
  syncCursor: SyncCursor
  data: ClientData
  userId: number | null
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
      syncCursor: { tables: {} },
      data: { tables: {} },
      userId: null,
    },
  ])
  const [activeTab, setActiveTab] = useState<'messages' | 'tableau'>('messages')
  const clientsRef = useRef<Client[]>([])
  const initialClientConnectedRef = useRef(false)
  
  // Keep ref in sync with state
  useEffect(() => {
    clientsRef.current = clients
  }, [clients])
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

  const performSyncCatchup = useCallback(
    async (clientId: string, sessionId: string, session: any) => {
      try {
        // Get sync cursor from ref (latest state)
        const client = clientsRef.current.find((c) => c.id === clientId)
        const syncCursor = client?.syncCursor || { tables: {} }
        
        // Build URL with query params
        const syncCursorParam = encodeURIComponent(JSON.stringify(syncCursor))
        const url = `http://localhost:3000/sync?sessionId=${sessionId}&syncCursor=${syncCursorParam}`
        const method = 'GET'
        
        addEvent({
          type: 'query_sent',
          data: { url, syncCursor, method },
          clientId,
        })

        const response = await fetch(url, {
          method,
        })

        if (!response.ok) {
          throw new Error(`Sync failed: ${response.statusText}`)
        }

        const syncResult = await response.json()

        // Update client data and sync cursor
        setClients((prev) =>
          prev.map((c) => {
            if (c.id !== clientId) return c

            const updatedCursor: SyncCursor = { ...c.syncCursor, tables: { ...c.syncCursor.tables } }
            const newData: ClientData = { tables: { ...c.data.tables } }

            // Process sync results - store all tables generically
            for (const [tableName, tableData] of Object.entries(syncResult.tables)) {
              const table = tableData as {
                rows: any[]
                permission_hash: string
                last_seen_updated_at: number | null
              }

              // Update cursor (preserve existing cursor data, update this table)
              updatedCursor.tables[tableName] = {
                last_seen_updated_at: table.last_seen_updated_at,
                permission_hash: table.permission_hash,
              }

              // Store table data generically - replace all rows for this table
              newData.tables[tableName] = table.rows
            }

            return {
              ...c,
              syncCursor: updatedCursor,
              data: newData,
            }
          })
        )

        addEvent({
          type: 'query_response',
          data: { message: 'Sync catchup completed', syncResult },
          clientId,
        })
      } catch (error: any) {
        addEvent({
          type: 'query_response',
          data: { error: error.message },
          clientId,
        })
      }
    },
    [addEvent]
  )

  const handleSyncDelta = useCallback(
    (clientId: string, deltaData: any) => {
      setClients((prev) =>
        prev.map((c) => {
          if (c.id !== clientId) return c

          const { all_affected_rows, affected_row_indices } = deltaData
          const updatedData: ClientData = { tables: { ...c.data.tables } }

          // Process affected rows - handle any table generically
          for (const index of affected_row_indices) {
            const affectedRow = all_affected_rows[index]
            if (!affectedRow) continue

            const { table_name, row } = affectedRow

            // Ensure table exists in data structure
            if (!updatedData.tables[table_name]) {
              updatedData.tables[table_name] = []
            }

            // Find existing row by id (assuming all tables have an id field)
            const rowIndex = updatedData.tables[table_name].findIndex((r: any) => r.id === row.id)
            if (rowIndex >= 0) {
              // Update existing row
              updatedData.tables[table_name][rowIndex] = { ...updatedData.tables[table_name][rowIndex], ...row }
            } else {
              // Add new row
              updatedData.tables[table_name].push(row)
            }
          }

          return {
            ...c,
            data: updatedData,
          }
        })
      )
    },
    []
  )

  const connectClient = useCallback((clientId: string) => {
    // Check current state from ref to avoid stale closures
    const client = clientsRef.current.find((c) => c.id === clientId)
    if (!client || client.connected || client.ws) return

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
          const session = message.session || { userId: null, role: 'user' }
          setClients((prev) =>
            prev.map((c) =>
              c.id === clientId
                ? { 
                    ...c, 
                    sessionId: message.sessionId, 
                    connected: true,
                    userId: session.userId || null,
                  }
                : c
            )
          )
          addEvent({
            type: 'query_response',
            data: { message: 'Connected', sessionId: message.sessionId },
            clientId,
          })
          // Perform sync catchup after state update
          setTimeout(() => {
            performSyncCatchup(clientId, message.sessionId, session)
          }, 0)
        } else if (message.type === 'delta') {
          addEvent({
            type: 'sync_delta',
            data: message.data,
            clientId,
          })
          // Handle sync delta
          handleSyncDelta(clientId, message.data)
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
  }, [performSyncCatchup, handleSyncDelta])

  // Connect WebSocket for initial client (only once, even in StrictMode)
  useEffect(() => {
    if (!initialClientConnectedRef.current) {
      initialClientConnectedRef.current = true
      connectClient('1')
    }
  }, [connectClient])

  const addNewClient = useCallback(() => {
    setClients((prev) => {
      const newId = `${prev.length + 1}`
      const newClient: Client = {
        id: newId,
        name: `Client ${newId}`,
        ws: null,
        sessionId: null,
        connected: false,
        syncCursor: { tables: {} },
        data: { tables: {} },
        userId: null,
      }
      // Connect immediately after adding
      setTimeout(() => {
        connectClient(newId)
      }, 0)
      return [...prev, newClient]
    })
  }, [connectClient])

  // Connect WebSocket for initial client (only once, even in StrictMode)
  useEffect(() => {
    if (!initialClientConnectedRef.current) {
      initialClientConnectedRef.current = true
      connectClient('1')
    }
  }, [connectClient])

  const submitQuery = useCallback(
    async (queryId: string, params: Record<string, any>) => {
      if (!selectedClientId) {
        throw new Error('No client selected')
      }

      const client = clients.find((c) => c.id === selectedClientId)
      if (!client) {
        throw new Error('Client not found')
      }

      const url = `http://localhost:3000/db/${queryId}`
      const method = 'POST'
      
      // Log query sent
      addEvent({
        type: 'query_sent',
        data: { queryId, params, url, method },
        clientId: selectedClientId,
      })

      try {
        const response = await fetch(url, {
          method,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(params),
        })

        const result = await response.json()

        addEvent({
          type: 'query_response',
          data: result,
          clientId: selectedClientId,
        })

        // Return result for QueryForm to display
        return result
      } catch (error: any) {
        const errorResult = { error: error.message }
        addEvent({
          type: 'query_response',
          data: errorResult,
          clientId: selectedClientId,
        })
        // Re-throw so QueryForm can catch and display
        throw error
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
          <div className="right-panel-tabs">
            <button
              className={`tab-button ${activeTab === 'messages' ? 'active' : ''}`}
              onClick={() => setActiveTab('messages')}
            >
              Messages
            </button>
            <button
              className={`tab-button ${activeTab === 'tableau' ? 'active' : ''}`}
              onClick={() => setActiveTab('tableau')}
            >
              Tableau
            </button>
          </div>
          <div className="right-panel-content">
            {activeTab === 'messages' ? (
              <MessagePane events={events} clients={clients} />
            ) : (
              <Tableau clients={clients} />
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

export default App
