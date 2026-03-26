# Namespacing

This guide explains how Pyre namespaces work today, how to structure files, and when to qualify references.

## What a namespace is

- A namespace is a schema partition name (for example `App` or `Auth`).
- Namespaces are discovered from folder names under `pyre/schema/`.
- If you use a single `pyre/schema.pyre` file, Pyre treats it as the default namespace.

## Project layouts

Single-schema project:

```text
pyre/
  schema.pyre
  queries.pyre
```

Multi-namespace project:

```text
pyre/
  schema/
    App/
      schema.pyre
    Auth/
      schema.pyre
  queries.pyre
```

## CLI behavior

- With one schema (`pyre/schema.pyre`), do not pass `--namespace`.
- With multiple schema folders, pass `--namespace <Name>` for namespace-specific operations (especially migrations).
- If you pass a namespace that does not exist in the filesystem layout, Pyre exits with an error.
- If a database already has Pyre metadata for a different namespace, migration commands fail with guidance.

## Reference rules inside schema files

### Unqualified links

`@link(authorId, User.id)` resolves `User` in the current namespace.

### Cross-namespace links

Use fully-qualified form: `Namespace.Record.field`.

```pyre
record Post {
    id Int @id
    authorId Int
    author @link(authorId, Auth.User.id)
    @public
}
```

If the namespace exists but that table does not exist in that namespace, typecheck fails.

## Naming rules and current constraints

- Non-default namespace names must be capitalized (`App`, not `app`).
- Record/type names are global across the loaded database, not per-namespace. In practice, this means you should avoid defining the same record/type name in two namespaces.
- Prefer one `session { ... }` definition across all namespaces to avoid ambiguity.

## Practical checklist

- Use `pyre/schema.pyre` for single-namespace projects.
- Use `pyre/schema/<Namespace>/schema.pyre` when splitting domains.
- Use `@link(..., Table.field)` for same-namespace links.
- Use `@link(..., Namespace.Table.field)` for cross-namespace links.
- When migrating multi-namespace projects, always pass `--namespace`.
