# Sync Playground

A simple React web app for testing Pyre's sync functionality with multiple WebSocket clients.

## Features

- **Multi-client support**: Create and manage multiple WebSocket clients, each with their own session
- **Query form**: Select queries/mutations, enter parameters, and execute them
- **Live message stream**: See queries, responses, and sync deltas from all clients in real-time
- **Automatic setup**: Database migration and seeding happen automatically on first run

## Setup

1. **Install dependencies**:
   ```bash
   bun install
   ```

2. **Start the development servers**:
   ```bash
   bun run dev
   ```

   This will:
   - Generate TypeScript code from Pyre files
   - Create `test.db` if it doesn't exist
   - Run migrations automatically
   - Seed the database with sample data
   - Start the backend server on `http://localhost:3000`
   - Start the Vite dev server on `http://localhost:5173`

3. **Open your browser**:
   Navigate to `http://localhost:5173` to see the React app

## Usage

### Adding Clients

Click the "+ Add Client" button in the header to create a new WebSocket connection. Each client gets a randomly generated `userId` (1-100) for testing.

### Running Queries

1. Select a client from the client list (left panel)
2. Choose a query/mutation from the dropdown
3. Fill in any required parameters
4. Click "Submit Query" to execute

### Viewing Messages

The right panel shows all messages received by all clients:
- **query_sent**: When a query is submitted
- **query_response**: The response from the server
- **sync_delta**: Sync deltas broadcast to clients based on permissions

### Testing Sync

1. Start the server and open the app in your browser
2. Create a post using the `createPost` mutation (make sure `published` is `false`)
3. Add another client (they'll have a different `userId`)
4. Update the post to `published: true` using the `updatePost` mutation
5. Watch the message stream - you should see sync deltas broadcast to clients based on permissions

## Project Structure

```
sync/
├── pyre/
│   ├── schema.pyre          # Database schema
│   ├── queries/             # Query/mutation files
│   └── generated/           # Generated TypeScript code
├── src/
│   ├── server.ts           # Hono server with WebSocket support
│   ├── client/
│   │   └── queryDiscovery.ts # Server-side query metadata discovery
│   ├── queryDiscovery.ts   # Client-side query discovery (fetches from API)
│   ├── App.tsx             # Main React app component
│   ├── main.tsx            # React entry point
│   └── components/         # React components
│       ├── ClientList.tsx
│       ├── QueryForm.tsx
│       └── MessagePane.tsx
├── index.html              # HTML entry point
├── vite.config.ts          # Vite configuration
└── test.db                 # SQLite database
```

## Architecture

### Server (`src/server.ts`)

- Handles HTTP requests for queries (`POST /db/:queryId`)
- Exposes query metadata endpoint (`GET /queries`)
- Manages WebSocket connections (`WS /sync`)
- Calculates and broadcasts sync deltas when mutations occur
- Automatically initializes database on first run

### Client (`src/App.tsx` + components)

- React-based web app
- Manages multiple WebSocket clients
- Fetches available queries from the server API
- Displays query results and sync deltas in real-time

## Development

- **Dev servers**: `bun run dev` (starts both backend and frontend)
- **Generate**: `bun run generate` (regenerate TypeScript from Pyre files)
- **Migrate**: `bun run migrate` (run migrations manually if needed)
- **Build**: `bun run build` (build for production)
- **Preview**: `bun run preview` (preview production build)

## Notes

- Each WebSocket client gets a randomly generated `userId` (1-100) for testing
- Sync deltas are calculated based on permissions defined in the schema
- The app automatically discovers queries by fetching from the server API
- Query IDs are hash-based and mapped to human-readable names
