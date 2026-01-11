import React from 'react'
import './Tableau.css'

interface Client {
  id: string
  name: string
  userId: number | null
  data: {
    user: any | null
    posts: any[]
  }
}

interface TableauProps {
  clients: Client[]
}

export default function Tableau({ clients }: TableauProps) {
  const getUserName = (client: Client): string => {
    if (client.data.user?.name) {
      return client.data.user.name
    }
    if (client.data.user?.email) {
      return client.data.user.email
    }
    return client.name
  }

  return (
    <div className="tableau">
      <h2>Tableau</h2>
      <div className="tableau-grid">
        {clients.map((client) => (
          <div key={client.id} className="tableau-client-wrapper">
            <div className="tableau-client-name">{getUserName(client)}</div>
            <div className="tableau-client-card">
              <div className="tableau-posts-grid">
              {client.data.posts.length === 0 ? (
                <div className="tableau-empty">No posts</div>
              ) : (
                client.data.posts.map((post) => (
                  <div key={post.id} className="tableau-post-card">
                    <div className="tableau-post-title">{post.title || 'Untitled'}</div>
                  </div>
                ))
              )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
