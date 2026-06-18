# Pyre

A schema and query language for building typesafe persistence using SQLite.

## Repository Map

- `src/` - Core Rust engine (parser, typechecker, SQL/code generation, sync)
- `pyre-cli/` - CLI entrypoint and subcommands (`pyre check`, `pyre generate`, `pyre migrate`, ...)
- `tests/` - Integration-style Rust tests, grouped by feature area (`parsing`, `queries`, `formatting`, ...)
- `packages/` - TypeScript runtime packages (`@pyre/core`, `@pyre/server`, `@pyre/client`)
- `wasm/` - WASM build and bindings
- `playground/` - Example projects and local experimentation setups
- `docs/usage/` - End-user setup and usage guides
- `docs/dev/` - Build and contributor-focused docs
- `docs/spec/` - Language and SQL generation specs

## Pre-requisites

You'll need Rust, Cargo and iconv installed.

Or you can use [devbox](https://www.jetify.com/devbox) to get all the right deps without polluting your system:

```
devbox shell
```

## Getting Started

```
cargo run
```

Useful CLI docs commands:

```bash
pyre docs
pyre docs schema
pyre docs query
pyre docs serve
pyre docs mcp
```

Built-in docs are also available under `docs/usage/`.

Recommended reading order:

1. `docs/usage/getting-started.md`
2. `docs/usage/schema.md`
3. `docs/usage/query.md`
4. `docs/usage/migrations.md`
5. `docs/usage/pyre-serve.md` or `docs/usage/sync.md`

## Examples

- `playground/simple/`
- `playground/sync/`
