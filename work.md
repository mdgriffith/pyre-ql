# npm Packaging + Monorepo Plan (Bun)

Goal: prepare Pyre for npm publishing with clear package boundaries and Bun workspaces, while minimizing disruption to existing Rust + playground workflows.

## Target structure

- packages/client (current client-elm package, published as @pyre/client)
- packages/server (current wasm/server package, published as @pyre/server)
- packages/core (shared TS types/protocol helpers used by client + server + generated targets)
- packages/cli (optional npm entrypoint wrapper for CLI distribution)

Keep outside packages:

- src/ (Rust compiler/generator)
- wasm/ (Rust/wasm build machinery)
- playground/
- docs/

## Workspace tooling (Bun only)

- Use Bun workspaces at root via package.json with `workspaces` field.
- Keep lockfile in Bun format (`bun.lockb`).
- Use Bun scripts for install/build/typecheck/test.

## Packaging principles

- Publish only dist artifacts and explicit exports.
- Avoid deep imports across package internals.
- Generated TypeScript should import from public package names only (`@pyre/client`, `@pyre/server`, `@pyre/core`).
- Consumers should not need to compile wasm; wasm artifacts are prepared before publish.

## Phase plan

### Phase 1: Workspace scaffolding

1. Add root package.json with Bun workspace config (`packages/*`).
2. Create `packages/` directory.
3. Move `client-elm` to `packages/client` (rename package name to `@pyre/client`).
4. Move `wasm/server` to `packages/server` (keep package name `@pyre/server`).
5. Update local file dependencies in playgrounds/docs/scripts.
6. Run `bun install` + typecheck/build validation.

### Phase 2: Shared core package

1. Create `packages/core` for shared TS-only types/protocol contracts.
2. Move duplicated shared type definitions from client/server into core.
3. Update generated target imports to consume `@pyre/core` where appropriate.
4. Validate no circular dependency between packages.

### Phase 3: Publish hardening

1. Add consistent exports/types/main/module fields.
2. Add `files` allowlist for npm publish.
3. Add smoke tests that install published tarballs in temp fixtures.
4. Add release scripts with Bun (`bun run release:*`).

### Phase 4: Optional npm CLI package

1. Add `packages/cli` npm wrapper if we want `npx pyre` UX.
2. Decide distribution strategy (native binary fetch vs wasm/node fallback).
3. Document platform support matrix.

## Phase 4 progress (CLI + editor)

- [x] Added `packages/cli` scaffold (npm package `pyre`, `bin/pyre.js`, `postinstall` hook)
- [ ] Add platform binary artifacts/packages (darwin/linux/windows x64 + arm64)
- [ ] Wire `postinstall` to fetch/copy the right binary into `packages/cli/vendor/pyre`
- [ ] Update release pipeline to build and attach per-platform binaries before npm publish
- [ ] Add smoke tests for `npx pyre --help` on at least macOS + Linux CI

Editor publishing direction:

- Keep language engine/source in repo (Rust/grammar/shared logic).
- [x] Add `packages/editor-vscode` (VS Code/Cursor extension package).
- Add `packages/editor-zed` (Zed extension package) once APIs are mapped.
- Share protocol/types between editor packages via `@pyre/core` or a small editor-shared package.

## Immediate execution checklist

- [x] Phase 1 step 1: root Bun workspace package.json
- [x] Phase 1 step 2: create packages/
- [x] Phase 1 step 3: move client-elm -> packages/client
- [x] Phase 1 step 4: move wasm/server -> packages/server
- [x] Phase 1 step 5: update all local references
- [x] Phase 1 step 6: run build/typecheck for moved packages + playground sync

## Phase 2 progress

- [x] Phase 2 step 1: create `packages/core`
- [x] Phase 2 step 2: move shared schema/query types into `@pyre/core`
- [x] Phase 2 step 3: update generator outputs to import `@pyre/core` where applicable
- [x] Phase 2 step 4: validate dependency graph + builds (no circular deps introduced)

## Phase 3 progress

- [x] Phase 3 step 1: add consistent package metadata/entry fields across `@pyre/client`, `@pyre/server`, `@pyre/core`
- [x] Phase 3 step 2: add `files` allowlists + `publishConfig.access`
- [x] Phase 3 step 3: add tarball smoke install test via `scripts/release-smoke.mjs`
- [x] Phase 3 step 4: add Bun release scripts in root package (`release:check`, `release:pack`, `release:smoke`)

## Phase 3.5 polish

- [x] Add minimal package READMEs for `@pyre/core` and `@pyre/server`
- [x] Add `release:clean` script to remove `.artifacts`
