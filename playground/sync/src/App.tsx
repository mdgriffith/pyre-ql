import React, { useState, useEffect, useCallback, useRef } from 'react'
import QueryForm from './components/QueryForm'
import MessagePane from './components/MessagePane'
import Clients from './components/Clients'
import { discoverQueries, QueryMetadata } from './queryDiscovery'
import { PyreClient } from '@pyre/client-elm'
import { schemaMetadata } from '../pyre/generated/client/node/schema'
import './App.css'

interface Client {
  id: string
  name: string
  pyreClient: PyreClient | null
  connected: boolean
  userId: number | null
  requestedUserId: number | null // User-specified userId for connection
  sessionId: string | null
  indexedDbName: string | null
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
      pyreClient: null,
      sessionId: null,
      connected: false,
      userId: null,
      requestedUserId: 1, // First client always starts as userId 1
      indexedDbName: 'pyre-sync-playground-1',
    },
  ])
  const [activeTab, setActiveTab] = useState<'messages' | 'clients'>('clients')
  const clientsRef = useRef<Client[]>([])
  const initialClientConnectedRef = useRef(false)
  const nextUserIdRef = useRef<number>(2) // Next userId to assign (starts at 2 since 1 is taken)
  const pyreClientsRef = useRef<Map<string, PyreClient>>(new Map())

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

  const connectClient = useCallback(async (clientId: string) => {
    // Check current state from ref to avoid stale closures
    const client = clientsRef.current.find((c) => c.id === clientId)
    if (!client || client.connected || client.pyreClient) return

    // Use requestedUserId if set, otherwise use nextUserId
    let userId: number
    if (client.requestedUserId != null) {
      userId = client.requestedUserId
    } else {
      if (clientId === '1') {
        userId = 1
        setClients((prev) =>
          prev.map((c) =>
            c.id === clientId ? { ...c, requestedUserId: 1 } : c
          )
        )
      } else {
        userId = nextUserIdRef.current
        nextUserIdRef.current = userId + 1
        setClients((prev) =>
          prev.map((c) =>
            c.id === clientId ? { ...c, requestedUserId: userId } : c
          )
        )
      }
    }

    addEvent({
      type: 'query_sent',
      data: {
        message: 'Connecting to server...',
        url: 'ws://localhost:3000/sync',
        method: 'WS',
      },
      clientId,
    })

    const indexedDbName = `pyre-sync-playground-${clientId}`

    // Create PyreClient instance
    const pyreClient = new PyreClient({
      schema: schemaMetadata,
      server: {
        baseUrl: 'http://localhost:3000',
      },
      indexedDbName,
    })

    // Store in ref for cleanup
    pyreClientsRef.current.set(clientId, pyreClient)

    // Set up sync progress callback
    pyreClient.onSyncProgress((progress) => {
      if (progress.complete) {
        addEvent({
          type: 'query_response',
          data: { message: 'Sync complete', tablesSynced: progress.tablesSynced },
          clientId,
        })
      } else {
        addEvent({
          type: 'query_response',
          data: { message: `Syncing table: ${progress.table}`, tablesSynced: progress.tablesSynced },
          clientId,
        })
      }
    })

    const unsubscribeSession = pyreClient.onSession((sessionId) => {
      setClients((prev) =>
        prev.map((c) =>
          c.id === clientId
            ? {
              ...c,
              sessionId,
            }
            : c
        )
      )
    })

    try {
      // Initialize client storage
      await pyreClient.init()

      const sessionId = pyreClient.getSessionId()

      setClients((prev) =>
        prev.map((c) =>
          c.id === clientId
            ? {
              ...c,
              pyreClient,
              connected: true,
              userId: userId,
              sessionId,
              indexedDbName,
            }
            : c
        )
      )

      addEvent({
        type: 'query_response',
        data: { message: 'Connected', sessionId },
        clientId,
      })
    } catch (error: any) {
      unsubscribeSession()
      console.error('Failed to initialize PyreClient:', error)
      addEvent({
        type: 'query_response',
        data: { error: error.message || 'Failed to connect' },
        clientId,
      })
      // Clean up on error
      pyreClientsRef.current.delete(clientId)
    }
  }, [addEvent])

  // Connect SSE for initial client (only once, even in StrictMode)
  useEffect(() => {
    if (!initialClientConnectedRef.current) {
      initialClientConnectedRef.current = true
      connectClient('1')
    }
  }, [connectClient])

  const addNewClient = useCallback(() => {
    // Get the next user ID and increment BEFORE adding to state
    // This ensures we don't have race conditions with React batching
    const newUserId = nextUserIdRef.current
    nextUserIdRef.current = newUserId + 1

    setClients((prev) => {
      const newId = `${prev.length + 1}`
      const newClient: Client = {
        id: newId,
        name: `Client ${newId}`,
        pyreClient: null,
        sessionId: null,
        connected: false,
        userId: null,
        requestedUserId: newUserId, // Assign next sequential userId
        indexedDbName: `pyre-sync-playground-${newId}`,
      }
      // Connect immediately after adding
      setTimeout(() => {
        connectClient(newId)
      }, 0)
      return [...prev, newClient]
    })
  }, [connectClient])

  const updateClientUserId = useCallback((clientId: string, userId: number | null) => {
    setClients((prev) =>
      prev.map((c) =>
        c.id === clientId
          ? { ...c, requestedUserId: userId }
          : c
      )
    )
  }, [])

  const submitQuery = useCallback(
    async (queryId: string, params: Record<string, any>) => {
      if (!selectedClientId) {
        throw new Error('No client selected')
      }

      const client = clients.find((c) => c.id === selectedClientId)
      if (!client) {
        throw new Error('Client not found')
      }

      // Include sessionId in query params if client has one
      const sessionId = client.sessionId
      const url = sessionId
        ? `http://localhost:3000/db/${queryId}?sessionId=${sessionId}`
        : `http://localhost:3000/db/${queryId}`
      const method = 'POST'

      // Log query sent
      addEvent({
        type: 'query_sent',
        data: { queryId, params, url, method, sessionId },
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

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      // Disconnect all PyreClient instances
      for (const pyreClient of pyreClientsRef.current.values()) {
        pyreClient.disconnect()
      }
      pyreClientsRef.current.clear()
    }
  }, [])

  const resetAllClients = useCallback(async () => {
    if (!confirm('Are you sure you want to delete all clients and clear all IndexedDB data? This cannot be undone.')) {
      return
    }

    // Disconnect all clients first
    for (const [clientId, pyreClient] of pyreClientsRef.current.entries()) {
      if (pyreClient) {
        pyreClient.disconnect()
      }
    }

    // Wait for connections to close
    await new Promise(resolve => setTimeout(resolve, 200))

    // Delete all client databases
    const deletePromises: Promise<void>[] = []
    const dbNames: string[] = []
    for (const [clientId, pyreClient] of pyreClientsRef.current.entries()) {
      if (pyreClient) {
        const client = clientsRef.current.find((item) => item.id === clientId)
        if (client?.indexedDbName) {
          dbNames.push(client.indexedDbName)
          console.log(`[Reset] Will delete database for client ${clientId}: ${client.indexedDbName}`)
        }
        deletePromises.push(pyreClient.deleteDatabase().catch(err => {
          console.error(`Error deleting database for client ${clientId}:`, err)
        }))
      }
    }

    // Also collect database names from client IDs (in case clients aren't in ref yet)
    for (let i = 1; i <= 10; i++) {
      const dbName = `pyre-sync-playground-${i}`
      if (!dbNames.includes(dbName)) {
        dbNames.push(dbName)
      }
    }

    try {
      await Promise.all(deletePromises)
      console.log('[Reset] All client databases deleted via PyreClient')

      // Also try to delete databases directly by name as a fallback
      console.log('[Reset] Attempting direct deletion of databases:', dbNames)
      for (const dbName of dbNames) {
        try {
          const deleteRequest = indexedDB.deleteDatabase(dbName)
          await new Promise<void>((resolve) => {
            deleteRequest.onsuccess = () => {
              console.log(`[Reset] Directly deleted database: ${dbName}`)
              resolve()
            }
            deleteRequest.onerror = () => {
              console.warn(`[Reset] Failed to directly delete ${dbName}:`, deleteRequest.error)
              resolve() // Don't fail
            }
            deleteRequest.onblocked = () => {
              console.warn(`[Reset] Database deletion blocked: ${dbName}`)
              resolve() // Don't fail on blocked
            }
          })
        } catch (err) {
          console.warn(`[Reset] Exception deleting ${dbName}:`, err)
        }
      }
      console.log('[Reset] Database deletion complete')
    } catch (error) {
      console.error('[Reset] Error deleting databases:', error)
    }

    // Clear the ref
    pyreClientsRef.current.clear()

    // Reset state to initial state
    setClients([
      {
        id: '1',
        name: 'Client 1',
        pyreClient: null,
        sessionId: null,
        connected: false,
        userId: null,
        requestedUserId: 1,
        indexedDbName: 'pyre-sync-playground-1',
      },
    ])
    setSelectedClientId('1')
    setEvents([])
    nextUserIdRef.current = 2
    initialClientConnectedRef.current = false

    // Wait longer before reconnecting to ensure databases are fully deleted
    setTimeout(() => {
      connectClient('1')
    }, 1000)
  }, [connectClient])

  return (
    <div className="app">
      <header className="app-header">
        <h1>Pyre Sync Playground</h1>
        <button
          onClick={resetAllClients}
          className="reset-button"
          title="Delete all clients and clear IndexedDB"
        >
          Reset All
        </button>
      </header>
      <div className="app-content">
        <div className="left-panel">
          <div className="left-panel-tabs">
            <button
              className={`tab-button ${activeTab === 'clients' ? 'active' : ''}`}
              onClick={() => setActiveTab('clients')}
            >
              Clients
            </button>
            <button
              className={`tab-button ${activeTab === 'messages' ? 'active' : ''}`}
              onClick={() => setActiveTab('messages')}
            >
              Messages
            </button>
          </div>
          <div className="left-panel-content">
            {activeTab === 'messages' ? (
              <MessagePane events={events} clients={clients} />
            ) : (
              <Clients
                clients={clients}
                selectedClientId={selectedClientId}
                onSelectClient={setSelectedClientId}
                onAddClient={addNewClient}
              />
            )}
          </div>
        </div>
        <div className="right-panel">
          <QueryForm
            queries={queries}
            onSubmit={submitQuery}
            selectedClient={clients.find((c) => c.id === selectedClientId)}
          />
        </div>
      </div>
    </div>
  )
}

export default App
