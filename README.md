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

## Examples

- `playground/simple/`
- `playground/sync/`
