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
   - Usually provides a single `connect` bootstrap hook.
   - Lets `PyreClient` attach the Elm bridge.
   - Usually does not need a custom mutation handler.

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

- **GET `/sync?databaseId=...`** → `Sync.catchup(db, syncCursor, session, pageSize, databaseId)`
- **GET `/sync/events`** → stream deltas to connected clients
- **query route** (e.g. `POST /db/:queryId?databaseId=...`) → `Sync.run(db, queries, queryId, args, session, connectedClients, databaseId)`

If schema/migrations run after startup, reload schema cache before expecting sync to work.

## Client runtime setup

Create one `PyreClient` instance per browser app instance:

```ts
let mainDatabaseId = "main"

const client = await PyreClient.create({
  schema: schemaMetadata,
  indexedDbName: "my-app-pyre",
  debug: true,
  connect: async () => {
    const response = await fetch("http://localhost:3000/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      credentials: "include",
      body: JSON.stringify({ userId: 1 }),
    })

    const bootstrap = await response.json()
    mainDatabaseId = bootstrap.mainDatabaseId
    const { userId } = bootstrap

    return {
      server: {
        baseUrl: "http://localhost:3000",
        credentials: "include",
        liveSyncTransport: "sse",
        endpoints: {
          catchup: "/sync",
          events: "/sync/events",
          query: "/db",
        },
      },
      cacheNamespace: userId,
      session: {
        userId,
      },
    }
  },
  onError: (error) => console.error(error),
});

await client.setSyncedDatabases([mainDatabaseId])
```

Set `debug: true` if you want verbose runtime logging while debugging sync behavior. Leave it off in normal app usage.

If session-backed filters change, refresh the runtime session so active queries are re-evaluated:

```ts
client.setSession({ userId: 2 })
```

Use `client.run(databaseId, queryModule, input, callback)` for TypeScript-native consumers. For generated Elm clients, prefer `PyreClient.create({ connect, elm: { ... } })` so the runtime owns the port bridge.

### Auth-neutral server options

`PyreClient` does not prescribe an authentication scheme. The server should authenticate each request using its normal app mechanism, then build the server-side Pyre session from that authenticated request context.

For cookie-authenticated APIs, configure standard credentialed browser requests:

```ts
let mainDatabaseId = "main"

const client = await PyreClient.create({
  schema: schemaMetadata,
  cacheNamespace: userId,
  server: {
    baseUrl: "https://api.example.com",
    credentials: "include",
  },
})
```

For bearer tokens, API keys, or CSRF headers, configure headers:

```ts
const client = await PyreClient.create({
  schema: schemaMetadata,
  cacheNamespace: userId,
  server: {
    baseUrl: "https://api.example.com",
    credentials: "include",
    headers: async () => ({
      Authorization: `Bearer ${await getAccessToken()}`,
      "X-CSRF-Token": getCsrfToken(),
    }),
  },
})
```

`credentials` accepts the fetch credential modes: `"omit"`, `"same-origin"`, and `"include"`. `withCredentials: true` is still accepted as shorthand for `credentials: "include"`.

Headers are used for HTTP catchup and mutation requests. Native browser `EventSource` cannot send custom headers, so SSE live sync can only use cookies through `credentials: "include"`.

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

Minimal Elm bridge setup:

```ts
const client = await PyreClient.create({
  schema: schemaMetadata,
  indexedDbName: "my-app-pyre",
  connect: async () => {
    const response = await fetch("http://localhost:3000/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      credentials: "include",
      body: JSON.stringify({ userId: 1 }),
    })

    const bootstrap = await response.json()
    mainDatabaseId = bootstrap.mainDatabaseId
    const { userId } = bootstrap

    return {
      server: {
        baseUrl: "http://localhost:3000",
        credentials: "include",
        liveSyncTransport: "sse",
        endpoints: {
          catchup: "/sync",
          events: "/sync/events",
          query: "/db",
        },
      },
      cacheNamespace: userId,
      session: { userId },
    }
  },
  elm: {
    app,
    onError: (error) => console.error(error),
  },
})

await client.setSyncedDatabases([mainDatabaseId])
```

Default Elm bridge ports:

- outbound from Elm: `pyreStoreOut`
- inbound query results: `pyre_receiveQueryDelta`
- inbound sync state: `pyre_receiveSyncState`
- inbound mutation results: `pyre_receiveMutationResult`

Override port names only if your app uses different names.

## Elm port contract (recommended)

Elm → TS:

- `register`
- `update-input`
- `unregister`
- `mutate`

Generated `Pyre` now returns effects as data:

```elm
type Effect
    = NoEffect
    | Send Encode.Value
    | LogError Encode.Value
```

The host app should map `Send`/`LogError` to its own outgoing ports.

For standard writes, prefer the generated mutation modules in `Query.*`.

Pyre now generates default CRUD mutations for each table:

- `{Table}Create`
- `{Table}Update`
- `{Table}Delete`

That means Elm app code can usually initiate writes through generated modules like `Query.DocumentCreate`, `Query.DocumentUpdate`, and `Query.DocumentDelete` without authoring custom mutation queries first.

Reach for a handwritten mutation query only when the write is not simple CRUD, such as nested inserts or other custom write behavior.

Generated update mutation modules use `Db.Updates` for nullable update fields so Elm can distinguish:

- set a value
- leave the field unchanged
- set the field to `null`

`Db.Updates` exposes:

```elm
type Update a
    = Set a
    | Unchanged
    | SetToNull


set : a -> Update a
skip : Update a
null : Update a
object : List ( String, Update Encode.Value ) -> Encode.Value
```

Example update input for a generated `DocumentUpdate` mutation:

```elm
import Db.Updates


{ id = documentId
, description = Db.Updates.set "Updated description"
}
```

For generated update inputs:

- `Db.Updates.set value` sends the field with that value
- `Db.Updates.skip` omits the field from the encoded mutation input
- `Db.Updates.null` sends the field as JSON `null`

This is what allows single-column updates from Elm without conflating `null` and "unchanged".

Notes:

- Use generated `Pyre.elm` and `Query.*` modules as the Elm API surface.
- Let `PyreClient` handle the bridge protocol; app code should not construct register or mutate payloads by hand.
- Generated query shapes preserve filters, sorting, and limits automatically.

TS → Elm:

- Forward incoming query data to the generated `Pyre.decodeIncomingDelta` path.
- Forward incoming mutation results to the generated mutation module decoders.

Generated mutation modules expose `mutationRequest databaseId requestId input` and `decodeMutationResult`, so Elm can initiate mutations and handle results without needing to know the bridge payload format.

If you are bridging those generated messages into `@pyre/client`, prefer `await PyreClient.create({ ..., connect, elm: { ... } })`. That lets the client perform login or other bootstrap work, build the final server config, attach the built-in bridge automatically, execute standard mutations itself, and keep the host code close to app state.

## Things that are easy to miss

1. **CORS headers for custom headers**
   - If you send custom request headers, include them in `Access-Control-Allow-Headers`.
   - If you use `credentials: "include"`, configure CORS to allow credentials and use an explicit allowed origin.

2. **Session consistency**
   - Keep one session per runtime instance. Recreating sessions repeatedly can produce confusing behavior.

3. **Fail loudly on decode/contract mismatches**
   - Log query id/source and decode error details. Silent drops make sync debugging very hard.

4. **One source of truth for query identity**
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
