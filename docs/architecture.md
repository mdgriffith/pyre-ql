# Architecture

High-level map of how Pyre turns schema/query input into generated artifacts and runtime behavior.

## Repository Roles

- `src/` - Core Rust engine (AST, parser, typechecker, SQL/code generation, sync)
- `pyre-cli/` - CLI wrapper around core engine capabilities
- `wasm/` - WASM wrapper that exposes selected core functions to JS
- `packages/` - TypeScript runtime packages (`@pyre/core`, `@pyre/server`, `@pyre/client`)

## Flow 1: Schema Parse + Typecheck

1. CLI discovers `schema*.pyre` and query files (`pyre-cli/src/filesystem.rs`, re-exporting `src/filesystem.rs`).
2. Parser builds AST from source (`src/parser.rs`, `src/ast.rs`).
3. Typechecker validates records/types/links and builds `Context` (`src/typecheck.rs`).
4. `Context` drives downstream generation and runtime checks.

## Flow 2: Query Parse + SQL/Code Generation

1. Query files are parsed into `ast::QueryList` (`src/parser.rs`).
2. Each query is validated against schema context (`src/typecheck.rs`).
3. SQL + target artifacts are generated (`src/generate/`):
   - SQL builders in `src/generate/sql/`
   - TypeScript outputs in `src/generate/typescript/`
   - Elm client outputs in `src/generate/client/elm.rs`
4. CLI writes generated files to `pyre/generated/` (`pyre-cli/src/command/generate.rs`).

## Flow 3: Sync Pipeline

1. Typechecker assigns table dependency/sync layers (`src/typecheck.rs`).
2. Sync SQL/status logic is produced by sync modules (`src/sync.rs`, `src/sync_deltas.rs`).
3. Server/client targets consume generated metadata and SQL helpers (`src/generate/typescript/targets/`).
4. WASM wrapper exposes sync helpers to JS (`wasm/src/sync.rs`, `wasm/src/sync_deltas.rs`).

## WASM Integration

- `wasm/src/lib.rs` is the JS-facing entrypoint.
- It delegates to focused modules (`migrate.rs`, `query.rs`, `seed.rs`, `sync.rs`) that call into core `pyre`.
- Build artifacts are produced in `wasm/pkg/` and copied into `packages/server/wasm/` by `scripts/build`.

## Practical Edit Guide

- Syntax or parsing behavior: `src/parser.rs`, `src/ast.rs`
- Schema/query validation: `src/typecheck.rs`
- SQL generation logic: `src/generate/sql/`
- Generated TS API shape: `src/generate/typescript/`
- CLI behavior and file IO: `pyre-cli/src/`
- JS/WASM API behavior: `wasm/src/`
