# Multi-Database Devtools

## Goal

Pyre devtools should make one browser page with one or more `PyreClient` instances understandable.

The devtools must be client-side only. It should not require or imply a backend debug endpoint.

The primary use cases are:

- A normal app with one `PyreClient` that routes work across multiple source databases.
- A playground/debug app with multiple independent `PyreClient` instances on the same page, such as multiple users in the sync playground.
- An app that changes which databases are actively synced over time.

## Core Principle

Devtools must render explicit state owned by `PyreClient`.

Devtools must not infer Pyre runtime state from:

- DOM state
- event history
- cache name conventions
- direct IndexedDB reads
- presence or absence of IndexedDB databases

`PyreClient` owns all devtools-facing state, including:

- registered instances
- cache namespace and instance metadata
- known touched databases
- active sync set / flagged-for-sync state
- per-database sync lifecycle and scheduler status
- aggregate sync state
- mutation/server-operation events
- table summaries
- row query results for selected table inspection
- sync cursors and table statuses
- IndexedDB names

IndexedDB may be the underlying storage source, but only `PyreClient` or its internal services should read it. Devtools should receive normalized snapshots or event streams from the client.

## Terms

- **Instance**: one public/meta `PyreClient` object in the page.
- **Instance ID**: stable generated ID for a `PyreClient` object lifetime.
- **Cache namespace**: server-provided namespace used for local cache isolation. It is display metadata, not identity.
- **Database**: one source database identified by `databaseId`.
- **Known database**: a database ID the current `PyreClient` instance has touched during its lifetime.
- **Flagged for syncing**: a database ID currently present in the instance's active sync set, as set by `setSyncedDatabases`, `syncDatabase`, or `unsyncDatabase`.

## Registry Lifecycle

Devtools should support multiple `PyreClient` instances through a client-owned registry. The registry must not be an unbounded hidden global list.

Required behavior:

- Only public/meta `PyreClient` instances register as top-level devtools instances.
- Internal single-database clients must not register as top-level instances.
- Registration happens only after `PyreClient.create(...)` succeeds.
- Failed client creation must not leave a registry entry.
- `client.disconnect()` unregisters that instance from the registry.
- `setSyncedDatabases([])` does not unregister the instance. It only changes sync state.
- A client with no active databases can remain registered until disconnect.
- Each registered instance gets a stable generated `instanceId` for the lifetime of that `PyreClient` object.
- The registry supports multiple mounted devtools panels as read-only subscribers.
- Mounting or unmounting a panel must not create or destroy client instances.
- The registry exposes a test-only reset hook so tests can clear registered instances between cases.

Production/runtime constraint:

- Registration should be lightweight and safe even when no devtools panel is mounted.
- Heavy devtools UI code must stay behind the devtools entry point.
- Any always-loaded registry/snapshot code should be small.

## Instance Selection

The devtools UI should expose a top-level instance selector.

Instance identity uses `instanceId`. Human labels use:

1. Optional devtools label, if a future config adds one.
2. Cache namespace, if available.
3. Stable fallback such as `Instance 1`, `Instance 2`.

If multiple live instances share the same cache namespace, the UI should disambiguate labels with a suffix such as `user_1`, `user_1 (2)`.

The sync playground should rely on automatic registry behavior: each created playground client appears as a selectable instance in one devtools panel.

## Known Database Semantics

For the first implementation, known databases are runtime-scoped and client-owned.

Touching a database includes:

- calling `client.run(databaseId, ...)`
- passing the database to `setSyncedDatabases(...)`
- calling `syncDatabase(databaseId)`
- calling `unsyncDatabase(databaseId)`
- creating an internal client for that database
- receiving sync state or live messages for that database
- logging server-operation events for that database

Known databases should be append-only for the lifetime of a `PyreClient` instance. Unsyncing a database changes its sync flags/status, but it does not remove that database from devtools.

Known databases are cleared when the `PyreClient` disconnects and unregisters from the devtools registry.

Devtools should not discover databases by scanning IndexedDB, cache names, or backend state. Persisted-cache discovery is explicitly deferred. If that feature is added later, `PyreClient` must expose it as explicit client-owned state.

## Database Selection

For the selected instance, devtools should show all known databases.

Databases must not disappear just because they are no longer flagged for syncing. This is important for workflows where the app switches focus from one synced database to another.

The UI should expose a database selector scoped to the selected instance.

The table inspector should display table information for the selected database only. There should be no merged cross-database table view.

## Sync Visibility

Devtools should show both:

- Aggregated sync state for the selected instance.
- Per-database sync state for every known database.

For each known database, show at least:

- `databaseId`
- IndexedDB name
- Whether it is flagged for syncing
- Client-owned lifecycle status
- Raw sync state, if available
- Table-level sync statuses, if available
- Last known sync error, if any

Per-database lifecycle is classified by the public/meta `PyreClient`, not by DOM components.

Required lifecycle meanings:

- `not_started`: known database, no sync work has begun and it is not currently queued or active
- `queued`: flagged for sync and waiting behind another database in serialized catchup
- `syncing`: currently active in the scheduler/catchup/live startup path
- `live`: catchup complete and live sync active for that database
- `unsynced`: known database but not currently flagged for syncing
- `error`: latest known sync state for the database has an error

Suggested classification order:

```ts
if (latestError) lifecycle = "error";
else if (!flaggedForSync) lifecycle = "unsynced";
else if (syncingDatabaseId === databaseId) lifecycle = "syncing";
else if (completedSyncDatabaseIds.has(databaseId)) lifecycle = "live";
else if (syncedDatabaseIds.includes(databaseId)) lifecycle = "queued";
else lifecycle = "not_started";
```

The client should preserve raw sync state separately so the UI can show both lifecycle and table-level sync detail.

## Server Operation Logging

Operation logging is useful, but it is not the highest-priority devtools feature.

For the first implementation, focus on operations that go through the server. The most important case is mutations.

Required first-pass events:

- `mutation.started`
- `mutation.completed`
- `mutation.failed`

Each mutation event should include:

- instance ID
- database ID
- mutation ID
- mutation name, if available
- input payload
- result payload for completed mutations
- error for failed mutations
- timestamp
- elapsed time when available

Query subscription/result logging can be deferred or kept minimal. Sync-driven query result updates should not be treated as server operation logs.

Elm bridge mutations and TypeScript-native mutations should produce the same event shape where possible.

Custom mutation handlers may be logged as `mutation.custom_dispatched` if the client cannot observe their result/failure. Detailed custom-handler logging can be deferred.

Event retention guidance:

- Keep events in memory only.
- Cap by event count.
- Use a conservative default, such as 200 events per instance.
- Do not build advanced size accounting unless it is needed.
- Do not block core devtools work on payload optimization.

It is acceptable for the first implementation to ship without rich payload retention beyond recent server operation events.

## Table Inspection

Devtools should not load or retain full copies of all table data.

Table inspection should use an efficient client-owned querying path, similar in spirit to how the app queries local data.

For a selected instance and selected database, devtools should be able to request table data with controls such as:

- table name
- pagination cursor or offset/limit
- simple filters
- sort, if supported cheaply

The client/runtime should execute that inspection query against the selected database cache and return only the requested page.

Devtools should not directly read IndexedDB and should not eagerly fetch all tables or all rows for all known databases.

Instance/database snapshots can include table names, row counts, sync status, and cursors if cheaply available. Row data should be loaded lazily for the selected table/database.

If a known database has no initialized cache yet, show an explicit empty/not-initialized state rather than hiding it.

## Public API Shape

The common case should not require app code to manually register clients.

`PyreClient.create(...)` should register the instance with the devtools registry as part of successful creation.

`mountPyreDevtools(...)` should mount one panel that reads from the registry and can switch instances.

The existing ability to mount devtools for a specific client can be removed or adapted if the registry-based panel fully replaces it. No backwards-compatibility layer is required.

## Suggested Runtime Model

Each `PyreClient` should maintain or expose a devtools snapshot shaped around instances and databases:

```ts
type DevtoolsDatabaseLifecycle =
  | 'not_started'
  | 'queued'
  | 'syncing'
  | 'live'
  | 'unsynced'
  | 'error';

interface DevtoolsRegistrySnapshot {
  instances: DevtoolsInstanceSummary[];
}

interface DevtoolsInstanceSummary {
  instanceId: string;
  label: string;
  cacheNamespace?: string;
}

interface DevtoolsInstanceSnapshot {
  instanceId: string;
  label: string;
  cacheNamespace?: string;
  aggregateSyncState: SyncState;
  databases: DevtoolsDatabaseSummary[];
  events: PyreDevtoolsEvent[];
}

interface DevtoolsDatabaseSummary {
  databaseId: string;
  indexedDbName: string;
  flaggedForSync: boolean;
  lifecycle: DevtoolsDatabaseLifecycle;
  syncState?: SyncState;
  tableSummaries?: DevtoolsTableSummary[];
  error?: string;
}

interface DevtoolsTableSummary {
  name: string;
  count?: number;
  sync?: TableSyncStatus;
  cursor?: unknown;
}

interface DevtoolsTablePageRequest {
  instanceId: string;
  databaseId: string;
  tableName: string;
  offset?: number;
  limit?: number;
  filter?: unknown;
  sort?: unknown;
}

interface DevtoolsTablePage {
  rows: unknown[];
  offset: number;
  limit: number;
  hasMore: boolean;
}
```

The exact types can evolve during implementation, but snapshots should preserve these concepts.

## UX Requirements

The sync playground should not distort the normal app experience.

The UI should work well for both:

- one instance / one database
- multiple instances / multiple databases

Top-left controls should be:

1. Instance selector: which user/cache namespace/client instance is being observed.
2. Database selector: which database is being inspected for the selected instance.

If there is only one instance, the instance selector should be visually quiet but still present or clearly labeled.

If there is only one database, the database selector should be visually quiet but still show the selected database ID.

The rest of the panel should be scoped to the selected instance and selected database.

## Testing Requirements

Use whatever testing modality is needed to prove the multi-database devtools behavior is correct.

DOM-level tests are useful for rendering and interactions, but they are not sufficient by themselves. The most important correctness boundary is the client-owned devtools state produced by `PyreClient`.

Required test layers:

- Client/runtime model tests for devtools snapshots, registry lifecycle, known database tracking, lifecycle classification, operation event capture, and retention.
- DOM/component tests for instance selection, database selection, table rendering, operation event rendering, and empty/error states.
- Focused integration tests where useful, especially around the public `PyreClient` API paths that should update devtools state.

The implementation does not need full browser end-to-end tests unless a behavior cannot be validated with cheaper tests.

Tests should prioritize proving the source of truth is correct before proving the UI renders it.

Tests should cover:

- Multiple `PyreClient` instances appear in the top-level instance selector.
- Only public/meta clients register, not internal single-database clients.
- Failed client creation does not register an instance.
- `disconnect()` unregisters exactly once.
- Registry reset clears instances between tests.
- Multiple devtools panels can subscribe without creating duplicate instances.
- Instance labels prefer cache namespace and disambiguate duplicates.
- Switching instances changes visible databases, sync states, table summaries, and events.
- Known databases are added from query, mutation, sync, and sync-control paths.
- All known databases remain visible after a database is unsynced.
- A database flagged for syncing shows flagged state even while queued.
- Serialized sync scheduling is reflected as one `syncing` database and one or more `queued` databases.
- Lifecycle classification covers queued, syncing, live, unsynced, not-started, and error.
- Aggregated sync state and per-database sync state both render.
- Table inspector is scoped to the selected database only.
- Table row loading is lazy and uses client-owned inspection/query APIs.
- Mutation events include `databaseId`, phase, ID/name, input, result/error, timestamp, and elapsed time where applicable.
- Event retention drops oldest events when count cap is exceeded.
- Sync playground-style multiple instances can be registered without app-side manual registry calls.

DOM tests should use fake snapshots where possible so UI tests are stable and not dependent on IndexedDB or live sync. Runtime model tests should validate the real `PyreClient` devtools state collection.

## Acceptance Criteria

- Opening devtools in the sync playground shows one panel with instance and database selectors in the top-left area.
- Creating additional playground clients adds additional selectable instances automatically.
- Selecting an instance shows that instance's cache namespace and known databases.
- Selecting a database shows only that database's table summaries/cache inspection state.
- Unsyncing or switching active databases does not remove previously touched databases from devtools.
- Server mutations in the playground appear in the operation log with their `databaseId`.
- Devtools remains entirely client-side.
- Devtools does not directly read IndexedDB.
