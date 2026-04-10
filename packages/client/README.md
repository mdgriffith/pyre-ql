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
  debug: true,
  session: {
    userId: 1,
  },
});

await client.init();

// Or:
const readyClient = await PyreClient.create({
  schema: schemaMetadata,
  debug: true,
  server: {
    baseUrl: 'http://localhost:3000',
    endpoints: {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
    },
  },
});

Set `debug: true` to enable verbose runtime logging. By default, `@pyre/client` does not emit its internal `console.log` diagnostics.

// Or resolve async setup during create:
const factoryClient = await PyreClient.create({
  schema: schemaMetadata,
  connect: async () => {
    const response = await fetch('http://localhost:3000/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: 1 }),
    });

    const { sessionId } = await response.json();
    const queryParams = new URLSearchParams({ sessionId }).toString();

    return {
      server: {
        baseUrl: 'http://localhost:3000',
        endpoints: {
          catchup: `/sync?${queryParams}`,
          events: `/sync/events?${queryParams}`,
          query: `/db?${queryParams}`,
        },
      },
    };
  },
});

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

### Registering Bridge Queries

When bridging generated Elm query payloads into `PyreClient`, you can register a query with an explicit `queryId` and receive revisioned results back:

```typescript
const subscription = client.registerQuery(
  {
    queryId: 'user-1',
    queryName: 'GetUser',
    querySource,
    input: { id: 1 },
  },
  ({ queryId, queryName, revision, result }) => {
    console.log(queryId, queryName, revision, result);
  }
);

subscription.update({ id: 2 });
subscription.unsubscribe();
```

### Attaching An Elm Bridge

If your app already has Elm ports for Pyre messages, you can let `@pyre/client` own the bridge wiring:

```typescript
const client = await PyreClient.create({
  schema: schemaMetadata,
  connect: async () => {
    const response = await fetch('http://localhost:3000/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: 1 }),
    });

    const { sessionId } = await response.json();
    const queryParams = new URLSearchParams({ sessionId }).toString();

    return {
      server: {
        baseUrl: 'http://localhost:3000',
        liveSyncTransport: 'sse',
        endpoints: {
          catchup: `/sync?${queryParams}`,
          events: `/sync/events?${queryParams}`,
          query: `/db?${queryParams}`,
        },
      },
    };
  },
  elm: {
    app,
    onError: (error, context) => {
      console.error(context.phase, error);
    },
  },
});
```

`PyreClient.create(...)` automatically attaches the bridge when `elm` is provided.

If you need lower-level control, `client.attachElmBridge(...)` is still available.

Default ports:

- receive: `pyreStoreOut`
- query results: `pyre_receiveQueryDelta`
- sync state: `pyre_receiveSyncState`
- mutation results: `pyre_receiveMutationResult`

Pass port names only if your app uses different names.

This built-in bridge handles:

- `register`
- `update-input`
- `unregister`
- `mutate`
- forwarding revisioned query results back into Elm
- sending mutation requests to the server automatically
- forwarding mutation results back into Elm with `requestId`
- forwarding sync state back into Elm

Provide `elm.onMutation` only when you need to override that default mutation behavior.

### Sending Mutations

```typescript
client.run(CreatePost, { title: 'Hello' }, (result) => {
  console.log('Mutation result:', result);
});
```

For generated Elm mutation modules, send the generated request payload through your outbound port:

```elm
port pyreStoreOut : Encode.Value -> Cmd msg


sendCreatePost : Cmd msg
sendCreatePost =
    pyreStoreOut
        (Query.CreatePost.mutationRequest "create-post-1"
            { title = "Hello" }
        )
```

`PyreClient` will POST that mutation to the configured query endpoint and publish the result to `pyre_receiveMutationResult`.

## Ports

### Outgoing (Elm -> TypeScript)

- `requestInitialData`: Request all data from IndexedDB
- `writeDelta`: Write a delta to IndexedDB
- `connectSSE`: Connect to SSE endpoint
- `disconnectSSE`: Disconnect from SSE
- `queryResult`: Send query results (callbackPort, result)
- `mutationResult`: Send mutation results (`requestId`, `mutationId`, result)
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
- `receiveSendMutation`: Send a mutation (`requestId`, `mutationId`, baseUrl, input)

## Generated Elm integration

When using generated Elm query code, the intended setup is:

1. Keep `Pyre.Model` inside your Elm application model
2. Route `Pyre.Msg` through your app update function
3. Forward `Pyre.Send` payloads to your JS/TS host
4. Let `PyreClient` execute/register/update those queries
5. Send results and deltas back into Elm and decode them with `Pyre.decodeIncomingDelta`

Generated `Pyre.elm` already uses the generated `Query.*.queryShape` values when registering and updating queries, so application code does not need to look up metadata by query name.

Generated Elm mutation modules expose:

- `id`
- `name`
- `mutationRequest : String -> Input -> Encode.Value`
- `decodeMutationResult : Decode.Decoder MutationResult`

That lets Elm send a fully-specified mutation request with a caller-owned `requestId`, while `PyreClient` handles the HTTP request and live sync remains the read path.

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
