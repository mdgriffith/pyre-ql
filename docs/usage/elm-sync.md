# Elm + Sync Runtime Setup

This guide covers using Pyre sync in an Elm app where Elm UI talks to a TypeScript bridge over ports.

## Mental model

There are three layers:

1. **Elm app**
   - Owns UI state.
   - Registers/unregisters queries.
   - Receives query results/deltas.

2. **TypeScript bridge**
   - Hosts `PyreClient` from `@pyre/client`.
   - Manages session/login.
   - Forwards generated `Pyre.elm` effects to `PyreClient`.
   - Forwards runtime results back to Elm ports.

3. **Server sync runtime**
   - `@pyre/server/sync` routes (`/sync`, `/sync/events`, query route).
   - Computes catchup and live deltas.

Elm should not reimplement sync transport details. Keep transport/stateful runtime concerns in the TS bridge.

## Server requirements (non-optional)

At startup:

1. `await Sync.init()`
2. Initialize DB/migrations
3. `await Sync.loadSchemaFromDatabase(db)`

Routes:

- **GET `/sync`** → `Sync.catchup(db, syncCursor, session, pageSize)`
- **GET `/sync/events`** → stream deltas to connected clients
- **query route** (e.g. `POST /db/:queryId`) → `Sync.run(db, queries, queryId, args, session, connectedClients)`

If schema/migrations run after startup, reload schema cache before expecting sync to work.

## Client runtime setup

Create one `PyreClient` instance per browser app instance:

```ts
const client = new PyreClient({
  schema: schemaMetadata,
  server: {
    baseUrl: "http://localhost:3000",
    liveSyncTransport: "sse",
    endpoints: {
      catchup: `/sync?sessionId=${sessionId}`,
      events: `/sync/events?sessionId=${sessionId}`,
      query: `/db?sessionId=${sessionId}`,
    },
  },
  indexedDbName: "my-app-pyre",
  session: {
    userId: 1,
  },
  onError: (error) => console.error(error),
});

await client.init();
```

If session-backed filters change, refresh the runtime session so active queries are re-evaluated:

```ts
client.setSession({ userId: 2 })
```

Use `client.run(queryModule, input, callback)` for TypeScript-native consumers. For generated Elm clients, the preferred integration is forwarding generated `Pyre.Send` effects rather than mapping query names manually in application code.

Use `client.onSyncState(...)` for high-level sync lifecycle updates:

```ts
const unsubscribeSync = client.onSyncState((syncState) => {
  // "not_started" | "catching_up" | "live"
  if (syncState.status === "live") {
    // Initial catchup is complete, live sync is active,
    // and currently registered queries have been fulfilled
  }

  // Per-table status: "waiting" | "catching_up" | "live"
  console.log(syncState.tables)
})
```

`SyncState.error` is optional and reported separately from lifecycle transitions.

## Elm port contract (recommended)

Elm → TS:

- `register`
- `update-input`
- `unregister`

Generated `Pyre` now returns effects as data:

```elm
type Effect
    = NoEffect
    | Send Encode.Value
    | LogError Encode.Value
```

The host app should map `Send`/`LogError` to its own outgoing ports.

Example message:

```json
{
  "type": "register",
  "queryName": "ListUsers",
  "queryId": "users-1",
  "querySource": {
    "user": {
      "@where": { "ownerId": { "$session": "userId" } },
      "id": true,
      "name": true
    }
  },
  "queryInput": {}
}
```

Notes:

- `queryName` is for routing responses back into generated `Pyre.elm`
- `querySource` is the actual generated query shape
- generated query shapes preserve `@where`, `@sort`, and `@limit`
- `@where` placeholders are encoded as:
  - `{"$var":"fieldName"}` for query input
  - `{"$session":"fieldName"}` for session values

`PyreClient` resolves those placeholders before sending the query to the internal Elm runtime.

TS → Elm:

- Forward full result snapshots (or deltas if your Elm layer supports them), including:
  - `queryId`
  - `queryName`
  - `revision`
  - `result`

Wire incoming data through `Pyre.decodeIncomingDelta` from your app port subscription.

## Things that are easy to miss

1. **CORS headers for custom headers**
   - If you send custom request headers, include them in `Access-Control-Allow-Headers`.

2. **Session consistency**
   - Keep one session per runtime instance. Recreating sessions repeatedly can produce confusing behavior.

3. **Early callback race**
   - A query callback can fire immediately after registration. Ensure your registration map entry exists before processing callback state.

4. **Fail loudly on decode/contract mismatches**
   - Log query id/source and decode error details. Silent drops make sync debugging very hard.

5. **One source of truth for query identity**
   - Use generated `Pyre.elm` and `Query.*` modules. The host app should not look up TS metadata by query name manually.

## Troubleshooting checklist

- `/sync` returns 500
  - Check server logs first.
  - Verify `Sync.init()` and `Sync.loadSchemaFromDatabase(db)` ran.
  - Verify schema/migrations are current.

- Query route works but sync/catchup fails
  - Usually schema cache/runtime setup issue on server side.

- Elm shows default/empty state forever
  - Confirm TS bridge receives runtime callback.
  - Confirm bridge sends to Elm inbound port.
  - Confirm Elm decoder accepts payload shape.

- Live events never arrive
  - Confirm `/sync/events` connection stays open.
  - Confirm connected clients map is populated and used by `Sync.run(...).sync(...)`.
