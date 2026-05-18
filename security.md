# Pyre Security Assessment

This is an internal working document for security-sensitive parts of Pyre. It is not a public vulnerability disclosure policy. Its purpose is to make the current trust boundaries, risky files, and highest-priority fixes explicit.

## Security Model

Pyre generates database runtimes from trusted schema and query definitions. The generated SQL, generated server metadata, manifest files, sync permissions, and session values must be treated as server-controlled inputs.

Pyre does not currently provide a complete HTTP security layer. Applications integrating Pyre are responsible for authentication, route authorization, CSRF/CORS policy, tenant selection, and deciding which generated queries can be invoked by which callers.

## Files And Concepts That Need Care

Treat these as trusted code or privileged configuration:

- `manifest.json`: Generated query runtime metadata. If an attacker can alter this, they can alter SQL executed by the runtime.
- Generated query SQL files: Generated TypeScript/Rust query modules are executable database behavior, not inert data.
- `.pyre` schema files: Schema controls table names, permissions, sync rules, and generated code.
- `.pyre` query files: Queries control generated SQL and exposed query/mutation IDs.
- `session`: Server-owned authorization data. It must be derived by the server from authenticated state. Never trust a browser-provided `session` object or let clients set fields such as `userId`, `role`, `tenantId`, or `workspaceId`.
- `databaseId`: Server-owned routing/tenant selection data. It must be selected, validated, or allowlisted by the server for the authenticated caller. Client-provided `databaseId` can be treated as a request hint only after authorization.
- `syncCursor`: Comes from the client and must be treated as untrusted state. The core now rejects unknown tables and oversized permission hashes, but HTTP handlers should still cap request body bytes before JSON parsing.
- `pageSize`: Comes from the caller in server sync APIs and should have sane defaults and hard caps.
- SSE/WebSocket live sync endpoints: They stream data into local client cache. They need the same auth and tenant controls as normal query endpoints.
- IndexedDB cache contents: Synced data persists in the browser. Sensitive data synced to Pyre should be considered locally stored data.

## Highest Priority Risks

### 1. Identifier Quoting And Escaping

Status: partially addressed. `quote` and `single_quote` now escape embedded quotes, and `@tablename` values are typechecked with the centralized `is_safe_sql_identifier` policy. More call sites should use this same policy for schema names, aliases, attached DB names, and any future string-based naming directives.

Previously, identifier and single-quote helpers wrapped strings but did not escape embedded quote characters:

- `src/ext/string.rs`: `quote` and `single_quote`
- `src/parser.rs`: `@tablename("...")` accepts arbitrary string literals
- `src/generate/sql/to_sql.rs`: table/schema/column rendering depends on quoted identifiers
- `src/sync.rs`: sync SQL also renders table names and literals

This is high priority because generated SQL assumes schema-controlled names are safe. Normal parsed identifiers may be constrained elsewhere, but string-based directives such as `@tablename` need explicit validation or correct escaping.

Suspicious example:

```pyre
record User {
  @tablename("users\" where 1 = 1 --")
  id: Id.Int<User>
}
```

Even if this fails later, malformed identifier SQL is a security smell. The safer rule is to reject invalid table/schema/column names at parse or typecheck time, and also make SQL identifier quoting escape `"` as `""`.

Remaining direction:

- Add a single identifier validation path for table names, schema names, column names, aliases, and attached database names.
- Escape embedded double quotes in identifier quoting.
- Escape embedded single quotes in any helper that emits SQL string literals.
- Add tests for malicious `@tablename`, schema names, column names, and attached DB names.

### 2. Session Values Baked Into Sync Permission SQL

Status: addressed for the Rust and TypeScript server sync runtimes. Sync status/data SQL now carries bind parameters for session permission values, and the server runtimes execute those parameters separately from SQL text.

Normal generated query SQL uses parameters such as `$session_userId`, which the runtime binds. Sync permission SQL previously rendered session values directly into generated SQL literals.

Relevant code:

- `src/sync.rs`: `render_permission_where`
- `src/sync.rs`: `get_sync_status_sql`
- `src/sync.rs`: `get_sync_sql`
- `src/generate/sql/to_sql.rs`: `render_value`

Concrete example:

```pyre
session {
  userId: Int
}

record Post {
  id: Id.Int<Post>
  authorId: Int
  updatedAt: Int

  @allow(query) {
    authorId == Session.userId
  }
}
```

For a normal query, the permission filter should look conceptually like this:

```sql
where "post"."authorId" = $session_userId
```

The runtime then binds `$session_userId` separately.

In the old sync path, `render_permission_where` replaced the session variable with a literal before SQL execution. With `userId = 42`, the sync SQL became conceptually:

```sql
SELECT 'post' AS table_name,
       0 AS sync_layer,
       '...' AS permission_hash,
       NULL AS last_seen_updated_at,
       MAX("post".updatedAt) AS max_updated_at
FROM "post"
WHERE "post"."authorId" = 42
```

With a string session field, it became conceptually:

```sql
WHERE "post"."workspaceSlug" = 'acme'
```

`render_value` does escape single quotes for strings, so this is not automatically exploitable. The smell is that sync has a separate SQL generation path where untrusted session values are converted into SQL text instead of bound parameters. Any missed escaping path, unsupported type, future operator, function rendering, or identifier issue can turn this into SQL injection.

Implemented direction:

- Sync SQL generation returns SQL plus bind parameters for runtime execution.
- Permission filters use placeholders and bind session values in `src/server/sync.rs` and `packages/server/sync.ts`.
- Tests cover malicious session strings like `x' OR 1=1 --`.
- Avoid having separate query permission and sync permission SQL renderers unless they share the same escaping/parameterization guarantees.

### 3. Sync Workload Defaults And Caps

Status: partially addressed. Rust and TypeScript sync APIs now default to `1000` and cap effective page size at `5000`. The sync core rejects cursor entries for unknown tables and oversized permission hashes. Rust and TypeScript live sync runtimes now fall back to a small `syncRequired` message when a live delta exceeds row, payload byte, or recipient-count limits. HTTP request byte caps and connected session count remain integration follow-ups.

Sync can become expensive because it combines client-provided cursor state, page size, table scans, permission filters, JSON reshaping, WASM calls, batch database execution, and live delta fanout.

Relevant code:

- `src/server/sync.rs`: Rust `catchup` accepts `page_size`
- `packages/server/sync.ts`: TypeScript `catchup` defaults to `1000` and caps effective page size at `5000`
- `src/sync.rs`: `get_sync_status_sql` and `get_sync_sql`
- `packages/server/query-sync.ts`: live delta calculation and fanout

Current behavior:

- Rust rejects `page_size == 0` and caps effective page size at `5000`.
- TypeScript defaults to `1000` and caps effective page size at `5000`.
- `syncCursor` rejects unknown tables and permission hashes over `256` bytes.
- Rust and TypeScript live sync send `syncRequired` instead of a full delta when the reshaped delta exceeds `5000` rows, `1 MiB` serialized payload, or `1000` recipients.
- Connected session count is still integration-managed.

Recommended defaults:

- Default page size: `1000` rows per table is reasonable for development and moderate data.
- Hard max page size: start with `5000` or lower until load testing proves otherwise.
- Max sync cursor tables: should be bounded by known schema tables and reject unknown excessive keys.
- Max sync cursor byte size: enforce at HTTP boundary before parsing.
- Max connected sessions per process/tenant: integration-level cap.
- Max live delta payload bytes: send `syncRequired` so clients perform POST catchup instead of receiving oversized live deltas.

Recommended audit metrics:

- Number of tables included in sync status SQL.
- Number of tables needing sync.
- Page size requested and effective capped page size.
- Rows returned per table.
- SQL execution time for status and data queries.
- WASM reshape/delta calculation time.
- Live delta recipient count and payload bytes.
- IndexedDB write volume on the client.

## Additional Smells

### Server-Controlled Session

The session object is used for permissions and query args:

- `packages/server/query.ts`: validates and uses `executingSession`
- `packages/server/sync.ts`: passes `session` into WASM sync SQL generation
- `src/server/manifest.rs`: builds `PyreSession`

Do not pass client-provided session data directly into Pyre. The server should derive it from verified auth, for example a cookie/JWT/session store, and should strip fields the client is not allowed to control. A request body like `{ session: { userId: 123, role: "admin" } }` is always suspicious; the authenticated server context should be the source of those fields.

Unsafe pattern:

```ts
await run(db, queries, queryId, input, request.body.session);
```

Safer pattern:

```ts
const auth = await requireUser(request);
await run(db, queries, queryId, input, {
  userId: auth.user.id,
  role: auth.user.role,
  tenantId: auth.tenant.id,
});
```

### Server-Controlled Query Access

The runtime dispatches by query ID:

- `packages/server/query.ts`: `queryMap[queryId]`
- `src/server/query.rs`: `manifest.queries.get(query_id)`

Applications should not expose all generated queries to every authenticated user by default. Route handlers should allowlist query IDs, mutation IDs, and operations for each endpoint or caller class.

### Database ID And Tenant Isolation

Current `databaseId` helpers only require a non-empty string:

- `packages/server/database-id.ts`
- `src/server/database_id.rs`

If `databaseId` maps to a tenant, database file, schema, Turso database, cache namespace, or live sync channel, it must be validated against server-side authorization. Client-provided `databaseId` must not be enough to select another tenant's data.

Unsafe pattern:

```ts
const databaseId = new URL(request.url).searchParams.get("databaseId");
return catchup(dbFor(databaseId), cursor, session, pageSize, databaseId);
```

Safer pattern:

```ts
const auth = await requireUser(request);
const databaseId = await requireAuthorizedDatabaseId(auth, request);
return catchup(dbFor(databaseId), cursor, sessionFromAuth(auth), pageSize, databaseId);
```

### Sync Cursor Validation

The sync core now rejects cursor entries for tables that are not known synced tables and rejects permission hashes larger than `256` bytes. HTTP handlers should still enforce a maximum request body size before parsing JSON, because that is the only reliable way to prevent large JSON parse/memory costs.

### POST Body For Sync Cursor

The Elm catchup client sends `syncCursor` in a POST JSON body:

- `packages/client/src/Data/Catchup.elm`

This avoids putting cursor state in browser history, reverse proxies, access logs, observability tools, and referrers. It also lets HTTP handlers enforce body-size limits before JSON parsing.

Tradeoffs for POST catchup:

- Benefits: cursor moves out of URLs, larger cursors fit safely in the body, body-size middleware can reject oversized cursors, fewer accidental log/referrer leaks.
- Costs: changing existing endpoint semantics and being less cache-friendly than GET.
- Recommendation: use POST catchup only. Since there are no compatibility constraints right now, do not keep cursor-bearing GET catchup around.

### Credentialed Requests, CORS, And CSRF

Catchup and live sync can include credentials:

- `packages/client/src/Data/Catchup.elm`: `Http.riskyRequest`
- `packages/client/src-ts/service/sse.ts`: `EventSource` with credentials

If cookies are used, mutation endpoints need CSRF protection and strict CORS. Avoid `Access-Control-Allow-Origin: *` with credentials.

Guidelines:

- Use `SameSite=Lax` or `SameSite=Strict` cookies where possible.
- Require CSRF tokens for cookie-authenticated mutations and any POST catchup route that has side effects or can be abused for load.
- Allowlist origins explicitly when credentials are enabled.
- Do not reflect arbitrary `Origin` values into `Access-Control-Allow-Origin`.
- Treat SSE and WebSocket routes as authenticated data routes, not public event channels.

### Live Sync Fanout Limits

Live sync delta fanout is integration-sensitive. Pyre calculates permission-filtered deltas, but the application decides how many clients are connected and how messages are delivered.

Recommended levers:

- `maxConnectedSessionsPerProcess`: cap total sessions held by one process.
- `maxConnectedSessionsPerTenant`: prevent one tenant from consuming all fanout capacity.
- `maxDeltaRowsPerMutation`: split or drop deltas that exceed a row count.
- `maxDeltaPayloadBytes`: split, compress, or force clients to catch up when a delta is too large.
- `maxFanoutRecipientsPerMutation`: fall back to catchup notifications when too many clients need the same mutation.
- `syncDeltaTimeoutMs`: bound WASM delta calculation time and log slow paths.

Recommended fallback behavior for over-limit deltas: send a small `syncRequired`/`catchupRequired` style message to affected clients instead of broadcasting the full payload. Clients can then run normal catchup with page-size limits.

### Local Browser Cache

Pyre sync stores data in IndexedDB. Any synced table should be treated as browser-persisted data. Applications need clear logout/cache clearing behavior and should avoid syncing secrets unless local persistence is acceptable.

### VS Code Extension Binary Path

The VS Code extension currently references a hardcoded local binary path:

- `packages/editor-vscode/src/commands/errorCheck.ts`
- `packages/editor-vscode/src/commands/format.ts`

This is not shell injection because `execFile`/`spawn` are used with argv, but it is a packaging/supply-chain smell.

## Near-Term Action List

1. Apply centralized identifier validation to schema names, aliases, attached DB names, and future string-based naming directives.
2. Add HTTP body-size guidance/examples for sync cursor parsing.
3. Add live delta payload byte limits and fanout metrics.
4. Add adversarial tests for database IDs and large raw HTTP cursors.
5. Keep docs/examples aligned on POST catchup and avoid reintroducing cursor-bearing GET catchup.
