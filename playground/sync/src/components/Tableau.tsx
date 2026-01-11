import React from 'react'
import './Tableau.css'

interface Client {
  id: string
  name: string
  userId: number | null
  data: {
    tables: Record<string, any[]>
  }
}

interface TableauProps {
  clients: Client[]
}

export default function Tableau({ clients }: TableauProps) {
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
    <div className="tableau">
      <h2>Tableau</h2>
      <div className="tableau-grid">
        {clients.map((client) => {
          const posts = getPosts(client)
          return (
            <div key={client.id} className="tableau-client-wrapper">
              <div className="tableau-client-name">{getUserName(client)}</div>
              <div className="tableau-client-card">
                <div className="tableau-posts-grid">
                {posts.length === 0 ? (
                  <div className="tableau-empty">No posts</div>
                ) : (
                  posts.map((post) => (
                    <div key={post.id} className="tableau-post-card">
                      <div className="tableau-post-title">{post.title || 'Untitled'}</div>
                    </div>
                  ))
                )}
                </div>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
