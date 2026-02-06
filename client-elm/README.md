# Pyre Elm Client

A headless Elm application for Pyre data synchronization and querying. This client manages data in memory and communicates with IndexedDB and SSE via TypeScript ports.

## Architecture

- **Elm (`src/`)**: Manages in-memory state, executes queries, handles deltas, and sends mutations
- **TypeScript (`src-ts/`)**: Boots the Elm app and wires IndexedDB/SSE/query manager services

## Setup

1. Install dependencies:
```bash
npm install
```

2. Build Elm:
```bash
npm run build
# or for development:
npm run dev
```

3. Build TypeScript:
```bash
npm run typecheck
```

## Usage

### Initialization

```typescript
import { PyreClient } from '@pyre/client-elm';
import { schemaMetadata } from './generated/typescript/core/schema';

const client = new PyreClient({
  schema: schemaMetadata,
  server: {
    baseUrl: 'http://localhost:3000',
    endpoints: {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
    },
    headers: {
      Authorization: 'Bearer token',
    },
  },
  indexedDbName: 'pyre-client',
});

await client.init();
```

### Registering Queries

```typescript
const subscription = client.run(
  ListUsersAndPosts,
  {},
  (result) => {
    console.log('Query result:', result);
  }
);

subscription?.update({});
subscription?.unsubscribe();
```

### Updating Query Input

```typescript
subscription?.update({});
```

### Sending Mutations

```typescript
client.run(CreatePost, { title: 'Hello' }, (result) => {
  console.log('Mutation result:', result);
});
```

## Ports

### Outgoing (Elm -> TypeScript)

- `requestInitialData`: Request all data from IndexedDB
- `writeDelta`: Write a delta to IndexedDB
- `connectSSE`: Connect to SSE endpoint
- `disconnectSSE`: Disconnect from SSE
- `queryResult`: Send query results (callbackPort, result)
- `mutationResult`: Send mutation results (id, result)

### Incoming (TypeScript -> Elm)

- `receiveInitialData`: Receive all data from IndexedDB
- `receiveDelta`: Receive a synced delta
- `receiveSyncProgress`: Receive sync progress updates
- `receiveSyncComplete`: Receive sync complete notification
- `receiveSSEConnected`: Receive SSE connection confirmation
- `receiveSSEError`: Receive SSE error
- `receiveRegisterQuery`: Register a new query (queryId, queryShape, input, callbackPort)
- `receiveUpdateQueryInput`: Update query input (queryId, newInput)
- `receiveUnregisterQuery`: Unregister a query (queryId)
- `receiveSendMutation`: Send a mutation (id, baseUrl, input)

## Features

- ✅ In-memory data management
- ✅ Query execution against local state
- ✅ Automatic query re-execution on data changes
- ✅ Delta application and persistence
- ✅ HTTP mutation support
- ✅ SSE connection management
- ✅ IndexedDB persistence

## Limitations

- Query results with nested relationships are simplified (full JSON encoding needed)
- `$in` operator in filters needs proper list handling
- Dynamic port creation not supported (uses single queryResult port with routing)
