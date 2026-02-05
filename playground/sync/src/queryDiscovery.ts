export interface QueryMetadata {
  id: string // Query ID
  name: string // Human-readable name
  isMutation: boolean
  inputFields?: Array<{
    name: string
    type: 'string' | 'number' | 'boolean'
  }> // Parameter names with types
}

/**
 * Discover queries by fetching from the server API
 */
export async function discoverQueries(): Promise<QueryMetadata[]> {
  try {
    const response = await fetch('http://localhost:3000/queries')
    if (!response.ok) {
      throw new Error(`Failed to fetch queries: ${response.statusText}`)
    }
    const queries = await response.json()
    return queries
  } catch (error) {
    console.error('Failed to discover queries:', error)
    return []
  }
}
