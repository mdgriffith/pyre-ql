# Migration Guide

Pyre migrations apply schema changes to a SQLite-compatible database. In MCP workflows, migrations are usually handled with `pyre_generate_migration`, `pyre_migrate`, `pyre_db_status`, or the optional `database` argument to `pyre_init`.

## New Local Database

For a new project, `pyre_init` can create and migrate a local database when you provide a `database` path.

```json
{
  "dir": "pyre",
  "schema": "record User {\n    id Int @id\n}\n",
  "database": "pyre.db"
}
```

Pyre refuses to overwrite an existing database path during init.

## Existing Project

For an existing schema, first check the project:

```json
{
  "name": "pyre_check",
  "arguments": { "dir": "pyre" }
}
```

Then inspect database status:

```json
{
  "name": "pyre_db_status",
  "arguments": {
    "database": "pyre.db",
    "migration_dir": "pyre/migrations"
  }
}
```

Generate and apply migrations when needed:

```json
{
  "name": "pyre_generate_migration",
  "arguments": {
    "name": "add_users",
    "database": "pyre.db",
    "migration_dir": "pyre/migrations"
  }
}
```

```json
{
  "name": "pyre_migrate",
  "arguments": {
    "database": "pyre.db",
    "migration_dir": "pyre/migrations",
    "push": true
  }
}
```

## Namespaces

For namespaced schemas, pass `namespace` to migration-related tools so Pyre applies the intended schema partition.
