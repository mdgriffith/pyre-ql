# Rust Server Runtime

This guide is for wiring a Rust app server to Pyre using the native Rust server helpers instead of `@pyre/server`.

The app still runs `pyre generate`. Generated output includes `pyre/generated/manifest.json`, which powers dynamic query and mutation execution in Rust.

## Main Modules

```rust
pyre::server::manifest
pyre::server::query
pyre::server::schema
pyre::server::sync
```

## Startup

Load the generated manifest:

```rust
use pyre::server::manifest::Manifest;

let manifest = Manifest::load("pyre/generated/manifest.json")?;
```

Load the Pyre schema/context from the database:

```rust
use pyre::server::schema::load_schema_from_database;

let loaded_schema = load_schema_from_database(&conn).await?;
let context = loaded_schema.context()?;
```

Create the sync server:

```rust
use pyre::server::sync::SyncServer;

let sync_server = SyncServer::new(context);
```

## Session

Create a session from the logical app session shape defined in the Pyre schema:

```rust
use pyre::server::manifest::PyreSession;
use serde_json::json;

let session = PyreSession::new(
    json!({
        "userId": 1,
        "role": "admin"
    }),
    &manifest.session_schema,
)?;
```

`PyreSession` provides two views:

- `session.logical()` for sync permission checks
- `session.sql_args()` for query and mutation SQL execution

## Running Queries And Mutations

```rust
use pyre::server::query;
use serde_json::json;

let result = query::run(
    &conn,
    &manifest,
    query_id,
    json!({ "body": "hello" }),
    &session,
).await?;
```

`result.response` contains the query or mutation response JSON.

`result.affected_rows` contains mutation affected rows for live sync.

## Catchup Endpoint

For a `GET /sync` equivalent:

```rust
let sync_result = sync_server
    .catchup(
        &conn,
        &sync_cursor,
        session.logical(),
        1000,
    )
    .await?;
```

Return `sync_result` as JSON.

## Live Deltas After Mutations

After running a mutation:

```rust
let result = query::run(&conn, &manifest, query_id, input, &session).await?;

let messages = sync_server.calculate_deltas(
    &result.affected_rows,
    &connected_sessions,
)?;
```

Send each message to its session:

```rust
for item in messages {
    send_to_session(item.session_id, item.message);
}
```

`item.message` serializes as:

```json
{
  "type": "delta",
  "data": []
}
```

## Connected Sessions

Use `ConnectedSessions` for live delta permission filtering:

```rust
use pyre::server::sync::ConnectedSessions;

let connected_sessions: ConnectedSessions = /* session id -> logical session values */;
```

The concrete shape is:

```rust
HashMap<String, HashMap<String, pyre::sync::SessionValue>>
```

## Runtime Transformations

The Rust runtime handles these Pyre server transformations:

- JSON input stringification
- omittable `field__is_set` flags
- session SQL args as `session_<name>`
- per-statement SQL param filtering
- response formatting
- `_affectedRows` extraction

Do not reimplement these in the app server.

## Suggested Server Flow

1. Run `pyre generate` as part of the app build.
2. Load `pyre/generated/manifest.json` at server startup.
3. Load the Pyre schema context from the database.
4. Build `PyreSession` from the authenticated app session.
5. Use `query::run` for queries and mutations.
6. Return `result.response` to query/mutation callers.
7. After mutations, pass `result.affected_rows` to `SyncServer::calculate_deltas` and send live messages.
8. Use `SyncServer::catchup` for `/sync` catchup requests.

## Current Coverage

The Rust server helpers are covered by tests for:

- schema loading
- catchup sync
- live delta permission filtering
- generated insert/update/delete affected rows
- generated CRUD create/delete/update
- omitted vs explicit `null`
- JSON input serialization
- session argument binding
- manifest loading
- multi top-level query response formatting
- SQL parameter names with shared prefixes
