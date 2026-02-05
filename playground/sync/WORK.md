# Terminal Sync Playground - Implementation Plan

## Overview
This playground demonstrates Pyre's sync functionality with a terminal-based UI. It includes:
- A Hono server handling Pyre queries and WebSocket sync connections
- An Ink-based TUI client that can manage multiple WebSocket clients
- Automatic database setup (migration + seed) on first run

## Project Structure

```
terminal-sync/
├── pyre/
│   ├── schema.pyre          # Database schema
│   ├── migrations/          # Migration files (auto-generated)
│   └── queries/             # Query/mutation files
│       └── *.pyre          # Individual query files
├── src/
│   ├── server.ts           # Hono server with WebSocket support
│   └── client/
│       └── tui.ts          # Ink-based TUI client
├── test.db                 # SQLite database file
├── package.json
├── tsconfig.json
└── WORK.md                 # This file
```

## Implementation Steps

### Phase 1: Project Setup
1. **Create directory structure**
   - Create `playground/terminal-sync/` directory
   - Create `pyre/`, `pyre/queries/`, `pyre/migrations/` directories
   - Create `src/client/` directory

2. **Initialize package.json**
   - Use bun
   - Dependencies: `hono`, `@libsql/client`, `ink`, `react`, `ws` (or `@hono/ws`)
   - Dev dependencies: `@types/bun`, `@types/react`
   - Scripts: `dev` (server), `client` (TUI), `generate` (pyre generate)

3. **Create tsconfig.json**
   - Standard TypeScript config for bun
   - Include JSX support for Ink

### Phase 2: Database Schema & Queries
1. **Create schema.pyre**
   - Simple schema with session support
   - Example: User and Post records with permissions
   - Include `@watch` on Post for sync testing

2. **Create sample queries**
   - `queries/listUsers.pyre` - Query to list users
   - `queries/createUser.pyre` - Mutation to create user
   - `queries/createPost.pyre` - Mutation to create post
   - `queries/updatePost.pyre` - Mutation to update post (for sync testing)

3. **Database initialization**
   - On server start, check if `test.db` exists
   - If not, run `pyre migrate test.db` and `pyre generate`
   - Then seed the database (see Phase 3)

### Phase 3: Server Implementation (src/server.ts)

1. **Basic Hono setup**
   - Import Hono and WebSocket support
   - Set up database connection to `test.db`
   - Initialize WASM module for sync deltas

2. **Database initialization function**
   - Check if `test.db` exists
   - If not, run migration via `pyre migrate test.db`
   - Run `pyre generate` to generate TypeScript types
   - Seed database with initial data
   - Set schema in WASM cache

3. **Query endpoint** (`POST /db/:req`)
   - Similar to existing playgrounds
   - Accept query ID and args
   - Run query using generated TypeScript code
   - Return results

4. **WebSocket endpoint** (`WS /sync`)
   - Accept WebSocket connections
   - Generate unique session ID for each client
   - Store connected clients with their sessions
   - Handle connection/disconnection
   - Send sync deltas when mutations occur

5. **Mutation handling with sync**
   - When a mutation is executed:
     - Extract `_affectedRows` from result set
     - Convert to format expected by `calculate_sync_deltas`
     - Get all connected sessions
     - Call WASM `calculate_sync_deltas(affectedRows, connectedSessions)`
     - Broadcast deltas to appropriate clients based on groups

6. **Session management**
   - Each WebSocket connection gets a unique session ID
   - Generate session values (e.g., random userId) for testing
   - Store session mapping: `sessionId -> session values`

### Phase 4: TUI Client Implementation (src/client/tui.ts)

1. **Ink setup**
   - Set up React component structure
   - Create main App component
   - Handle terminal resize and exit

2. **Client management**
   - State: array of clients, each with:
     - `id`: unique identifier
     - `name`: display name (e.g., "Client 1", "Client 2")
     - `ws`: WebSocket connection
     - `sessionId`: server-assigned session ID
   - On startup, create first client automatically
   - "+" button to add new clients
   - Display client list at top

3. **Layout structure**
   ```
   ┌─────────────────────────────────────────┐
   │ [Client Dropdown ▼] [+ Add Client]      │ ← Nav bar
   ├──────────────────┬──────────────────────┤
   │                  │                      │
   │   REPL Panel     │   Event Stream       │
   │                  │                      │
   │   [Query Picker] │   [Event Log]       │
   │   [Params Form]  │                      │
   │   [Submit]       │                      │
   │   [Results]      │                      │
   │                  │                      │
   └──────────────────┴──────────────────────┘
   ```

4. **Query discovery**
   - Read `pyre/generated/typescript/targets/server/queries.ts`
   - Extract available queries from the switch statement
   - Map query IDs to query names (may need to generate metadata)
   - Display in dropdown/picker

5. **REPL Panel (left column)**
   - Query picker: dropdown/list of available queries
   - Parameter form: dynamically generated based on query Input type
   - Submit button: send query to server
   - Results display: show formatted JSON response

6. **Event Stream Panel (right column)**
   - Scrollable log of events
   - Event types:
     - `query_sent`: Show query ID + params
     - `query_response`: Show response data
     - `sync_delta`: Show WebSocket delta message (from any client)
   - Color coding: queries (blue), responses (green), deltas (yellow)

7. **WebSocket handling**
   - Connect to `ws://localhost:3000/sync` for each client
   - On connect, receive session ID from server
   - Listen for sync delta messages
   - Display deltas in event stream

8. **Query execution**
   - When query is submitted:
     - Log to event stream as `query_sent`
     - Send POST request to `/db/:queryId`
     - Log response as `query_response`
     - If mutation, server will handle sync delta broadcasting

### Phase 5: Query Metadata Generation

**Problem**: We need to map query IDs to human-readable names and know which queries are mutations vs queries.

**Solution Options**:
1. Parse `.pyre` query files to extract names
2. Generate metadata file during `pyre generate`
3. Use query ID as display name (not ideal)

**Chosen**: Parse query files from `pyre/queries/` directory:
- Read all `.pyre` files
- Parse query/mutation names (e.g., `query UserList`, `insert CreateUser`)
- Map to query IDs by matching with generated code
- Store metadata for TUI to use

### Phase 6: Seed Data

Create seed function that:
- Uses insert mutations to create initial data
- Creates a few users and posts
- Can be called programmatically from server startup

### Phase 7: Polish & Testing

1. **Error handling**
   - Handle WebSocket disconnections gracefully
   - Show errors in TUI
   - Handle query execution errors

2. **UI polish**
   - Better colors and styling
   - Keyboard shortcuts (Tab to switch panels, Enter to submit)
   - Loading states

3. **Documentation**
   - README with setup instructions
   - Usage guide

## Technical Details

### WebSocket Protocol
- Client connects to `ws://localhost:3000/sync`
- Server sends `{ type: 'connected', sessionId: '...' }` on connect
- Server sends `{ type: 'delta', data: SyncDeltasResult }` when mutations occur
- Client can send `{ type: 'ping' }` for keepalive

### Session Generation
- Generate random `userId` (1-100) for each client
- Store as: `{ userId: number, role: 'user' }`
- Send to server on WebSocket connect

### Sync Delta Format
From WASM `calculate_sync_deltas`:
```typescript
{
  all_affected_rows: Array<{
    table_name: string,
    row: Record<string, any>,
    headers: string[]
  }>,
  groups: Array<{
    session_ids: string[],
    affected_row_indices: number[]
  }>
}
```

### Affected Rows Extraction
From mutation result:
- Look for `_affectedRows` column in result sets
- Parse JSON array
- Each item has `{ table_name, row, headers }`

## Dependencies

### Server
- `hono` - Web framework
- `@hono/ws` or `ws` - WebSocket support
- `@libsql/client` - Database client
- WASM module from `../../wasm/pkg/pyre_wasm.js`

### Client
- `ink` - React-based TUI framework
- `react` - Required by Ink
- `ink-select-input` - Dropdown component
- `ink-text-input` - Text input component
- `ws` - WebSocket client

## Implementation Order

1. ✅ Create WORK.md (this file)
2. Set up project structure and package.json
3. Create schema and sample queries
4. Implement basic server (query endpoint only)
5. Add database initialization and seeding
6. Implement WebSocket server with sync delta broadcasting
7. Implement basic TUI structure
8. Add query discovery and REPL panel
9. Add event stream panel
10. Add WebSocket client integration
11. Polish UI and error handling
12. Test end-to-end flow

## Notes for Future AI

- The server needs to initialize WASM before use (see `playground/wasm-calling/syncDeltas.ts`)
- Query IDs are IDs, need to map them to names from query files
- Mutations return `_affectedRows` in a specific result set (not always the last one)
- Sync deltas are calculated per mutation, not continuously
- Each WebSocket client needs a unique session with proper session values
- The TUI should be responsive and handle terminal resizing
- Use Ink's built-in components where possible for consistency
