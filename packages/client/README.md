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
import { PyreClient } from '@pyre/client';
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
  session: {
    userId: 1,
  },
});

await client.init();

const unsubscribeSync = client.onSyncState((syncState) => {
  console.log(syncState.status); // "not_started" | "catching_up" | "live"
  // "live" means initial catchup is complete and current queries have been fulfilled
  console.log(syncState.tables); // Record<tableName, "waiting" | "catching_up" | "live">
});

// Optional legacy callback (derived from sync state)
client.onSyncProgress((progress) => {
  console.log(progress.complete);
});

// Refresh active queries when session-backed filters change
client.setSession({ userId: 2 });
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

`QueryShape` supports:

- selected fields
- `@where`
- `@sort`
- `@limit`

Generated query shapes can contain placeholders in `@where`:

- `{"$var":"fieldName"}` for query input values
- `{"$session":"fieldName"}` for client session values

`PyreClient` resolves those placeholders before sending the query to the internal Elm query engine.

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
- `syncStateOut`: High-level sync state (`status`, `tables`)

### Incoming (TypeScript -> Elm)

- `receiveInitialData`: Receive all data from IndexedDB
- `receiveDelta`: Receive a synced delta
- `receiveSyncProgress`: Receive sync progress updates
- `receiveSyncComplete`: Receive sync complete notification
- `receiveSSEConnected`: Receive SSE connection confirmation
- `receiveSSEError`: Receive SSE error
- `receiveRegisterQuery`: Register a new query (queryId, queryShape, input)
- `receiveUpdateQueryInput`: Update query input (queryId, queryShape, newInput)
- `receiveUnregisterQuery`: Unregister a query (queryId)
- `receiveSendMutation`: Send a mutation (id, baseUrl, input)

## Generated Elm integration

When using generated Elm query code, the intended setup is:

1. Keep `Pyre.Model` inside your Elm application model
2. Route `Pyre.Msg` through your app update function
3. Forward `Pyre.Send` payloads to your JS/TS host
4. Let `PyreClient` execute/register/update those queries
5. Send results and deltas back into Elm and decode them with `Pyre.decodeIncomingDelta`

Generated `Pyre.elm` already uses the generated `Query.*.queryShape` values when registering and updating queries, so application code does not need to look up metadata by query name.

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
