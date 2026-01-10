import React from 'react'
import './ClientList.css'

interface Client {
  id: string
  name: string
  connected: boolean
  sessionId: string | null
}

interface ClientListProps {
  clients: Client[]
  selectedClientId: string
  onSelectClient: (clientId: string) => void
}

export default function ClientList({
  clients,
  selectedClientId,
  onSelectClient,
}: ClientListProps) {
  return (
    <div className="client-list">
      <h2>Clients</h2>
      <div className="client-list-items">
        {clients.map((client) => (
          <div
            key={client.id}
            className={`client-item ${
              selectedClientId === client.id ? 'selected' : ''
            }`}
            onClick={() => onSelectClient(client.id)}
          >
            <div className="client-name">
              {client.name}
              {client.connected ? (
                <span className="status connected">✓</span>
              ) : (
                <span className="status disconnected">✗</span>
              )}
            </div>
            {client.sessionId && (
              <div className="client-session">{client.sessionId}</div>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
