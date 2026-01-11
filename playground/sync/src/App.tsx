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
  user: any | null
  posts: any[]
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
      data: { user: null, posts: [] },
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
            const newData: ClientData = { ...c.data }

            // Process sync results
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

              // Update data based on table name (handle both capitalized and lowercase)
              const tableNameLower = tableName.toLowerCase()
              if (tableNameLower === 'user' || tableNameLower === 'users') {
                // User table - should only have one row (the client's own user)
                if (table.rows.length > 0) {
                  newData.user = table.rows[0]
                }
              } else if (tableNameLower === 'post' || tableNameLower === 'posts') {
                // Posts table - replace all posts with synced data
                newData.posts = table.rows
              }
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
          const updatedData = { ...c.data }

          // Process affected rows
          for (const index of affected_row_indices) {
            const affectedRow = all_affected_rows[index]
            if (!affectedRow) continue

            const { table_name, row } = affectedRow

            if (table_name === 'User') {
              // Update user if it's the client's own user
              if (updatedData.user && updatedData.user.id === row.id) {
                updatedData.user = { ...updatedData.user, ...row }
              }
            } else if (table_name === 'Post') {
              // Update posts
              const postIndex = updatedData.posts.findIndex((p) => p.id === row.id)
              if (postIndex >= 0) {
                // Update existing post
                updatedData.posts[postIndex] = { ...updatedData.posts[postIndex], ...row }
              } else {
                // Add new post
                updatedData.posts.push(row)
              }
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
        data: { user: null, posts: [] },
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
      if (!selectedClientId) return

      const client = clients.find((c) => c.id === selectedClientId)
      if (!client) return

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
