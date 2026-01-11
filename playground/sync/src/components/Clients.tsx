import './Clients.css'

interface Client {
  id: string
  name: string
  userId: number | null
  requestedUserId: number | null
  connected: boolean
  data: {
    tables: Record<string, any[]>
  }
}

interface ClientsProps {
  clients: Client[]
  selectedClientId: string
  onSelectClient: (clientId: string) => void
  onAddClient: () => void
}

export default function Clients({
  clients,
  selectedClientId,
  onSelectClient,
  onAddClient,
}: ClientsProps) {
  const getUserName = (client: Client): string => {
    // Try to find user data in tables (handle both 'user' and 'users' table names)
    if (!client.data?.tables) {
      return client.name
    }
    const userTable = client.data.tables['user'] || client.data.tables['users']
    if (userTable && userTable.length > 0) {
      const user = userTable[0]
      if (user?.name) {
        return user.name
      }
      if (user?.email) {
        return user.email
      }
    }
    return client.name
  }

  const getPosts = (client: Client): any[] => {
    // Try to find posts data in tables (handle both 'post' and 'posts' table names)
    if (!client.data?.tables) {
      return []
    }
    return client.data.tables['post'] || client.data.tables['posts'] || []
  }

  return (
    <div className="clients">
      <h2>Clients</h2>
      <div className="clients-grid">
        {clients.map((client) => {
          const posts = getPosts(client)
          const isSelected = selectedClientId === client.id
          return (
            <div key={client.id} className="clients-client-wrapper">
              <div className="clients-client-name">
                {getUserName(client)} <span className="clients-client-id">({client.userId ?? client.requestedUserId ?? client.id})</span>
              </div>
              <div
                className={`clients-client-card ${isSelected ? 'selected' : ''}`}
                onClick={() => onSelectClient(client.id)}
              >
                <div className="clients-posts-grid">
                  {posts.length === 0 ? (
                    <div className="clients-empty">No posts</div>
                  ) : (
                    posts.map((post) => (
                      <div key={post.id} className="clients-post-card">
                        <div className="clients-post-title">{post.title || 'Untitled'}</div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          )
        })}
        <div className="clients-client-wrapper clients-add-client-wrapper">
          <div className="clients-add-client-card" onClick={onAddClient}>
            <div className="clients-add-client-text">Add client</div>
          </div>
        </div>
      </div>
    </div>
  )
}
