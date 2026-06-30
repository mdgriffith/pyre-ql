# Pyre - 0.1.2

`0.1.2` is a substantial update focused on making Pyre more usable end-to-end: better docs, a stronger CLI, a built-in server flow, seeding support, improved sync behavior, and clearer TypeScript/Elm package boundaries.

## Highlights

- Added `pyre serve` for running Pyre's built-in HTTP server against a local or remote libSQL database.
- Added `pyre docs` and an MCP server/tooling surface for agent-oriented workflows and project inspection.
- Added server-side seeding helpers for loading structured fixture/import data.
- Expanded sync support with better catch-up handling, optimistic mutation groundwork, entity delta streaming, and multi-database routing support.
- Added `@timestamps`, `@createdAt`, and `@updatedAt` helpers.
- Improved query, schema, and migration behavior with better correctness, more deterministic generation, and broader test coverage.
- Added npm packaging/release smoke-test infrastructure for `@pyre/core`, `@pyre/server`, and `@pyre/client`.

## New

### Built-in server

Pyre now includes `pyre serve`, a built-in single-database HTTP server for local development, demos, and simple deployments.

This provides standard endpoints for:
- health checks
- query execution
- sync catch-up
- live sync over SSE

### Docs and MCP support

Pyre now ships with:
- `pyre docs`
- bundled usage/spec documentation
- MCP tooling for structured access to docs, schema, checks, migrations, introspection, and query workflows

This should make Pyre much easier to inspect and drive from editor/agent workflows.

### Seeding

Added server-side seed support for inserting structured data shaped like the schema, including nested linked records.

This is useful for:
- fixtures
- local bootstrapping
- imports
- demos

### Sync and client runtime improvements

Sync behavior has been expanded significantly, including:
- improved catch-up handling
- persisted sync cursors
- server-authoritative revision IDs
- entity delta streaming
- groundwork for optimistic mutations
- support for routing a client across multiple backend databases

### Schema and modeling improvements

Added or improved support for:
- `@timestamps`
- `@createdAt`
- `@updatedAt`
- partial and compound indices
- multiline and chained permission conditions
- JSON handling
- tagged/custom type handling
- namespacing and multi-database workflows

## Improvements

- Improved formatting consistency and deterministic code generation.
- Improved query and migration diagnostics.
- Improved Rust server documentation and usage guides.
- Expanded generated TypeScript and Elm client/server support.
- Added release packaging and smoke-test scripts for published JS packages.
- Added a VS Code package layout and diagnostics-on-save fixes.

## Fixes

- Fixed nested insert behavior.
- Fixed several sync edge cases around catch-up and mutation propagation.
- Fixed JSON encoding/decoding correctness in multiple paths.
- Fixed client-side query alias handling.
- Fixed Elm generation issues and query-manager integration details.
- Fixed migration and introspection edge cases.
- Fixed multiple SQL generation issues across inserts, updates, permissions, and sync-related queries.

## Documentation

This release includes a major docs expansion, including:
- getting started
- schema
- query
- migrations
- sync
- Elm sync
- `pyre serve`
- MCP usage
- project structure
- troubleshooting
- multi-database upgrade guidance

## Notes

- `pyre serve` is currently designed for a single database per server process.
- Multi-database support is primarily a routing/runtime integration story and may require separate generated artifacts for separate schema families.
- npm publishing infrastructure has improved, but the full CLI binary distribution story is still evolving.

## Thanks

This release also adds a large amount of new automated test coverage across parsing, typechecking, migrations, queries, sync, MCP, and generated runtimes.
