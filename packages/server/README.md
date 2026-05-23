# @pyre/server

Server runtime helpers for executing generated Pyre queries.

Typical usage:

- import generated `queries` map from `pyre/generated/typescript/server`
- execute with `run` from `@pyre/server/query`
- use sync helpers from `@pyre/server/sync` and `@pyre/server/query-sync`

## Install

```bash
bun add @pyre/server
```

## Sync Lifecycle Profiling

Run a local in-memory profile:

```bash
bun run profile:sync
```

Run the same profile against Turso:

```bash
TURSO_DATABASE_URL=libsql://... \
TURSO_AUTH_TOKEN=... \
SYNC_PROFILE_ALLOW_REMOTE_WRITES=1 \
bun run profile:sync
```

Useful knobs:

- `SYNC_PROFILE_ROWS`, default `1000`
- `SYNC_PROFILE_PAGE_SIZE`, default `1000`
- `SYNC_PROFILE_ITERATIONS`, default `10`
- `SYNC_PROFILE_SESSIONS`, default `25`
- `SYNC_PROFILE_MIMIC_RTT_MS`, default `20`
- `SYNC_PROFILE_MIMIC_BANDWIDTH_MBPS`, default `25`

The profile creates an isolated `pyre_sync_profile_notes` table and reports total time, average time, and percentage by phase for catch-up and mutation-to-delta sync.
It also compares row-materialized catch-up with a SQLite aggregate JSON catch-up shape and prints a simple remote mimic estimate from measured DB payload bytes.
