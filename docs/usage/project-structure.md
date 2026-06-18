# Project Structure

This page shows the common filesystem layouts Pyre expects.

## Single-Schema Project

```text
your-project/
├── pyre/
│   ├── schema.pyre
│   ├── query.pyre
│   └── generated/
│       ├── client/
│       │   └── elm/
│       └── typescript/
└── db/
    └── playground.db
```

Notes:

- `schema.pyre` is the default single-schema location.
- Any non-schema `.pyre` file under `pyre/` is treated as a query file.
- `query.pyre` is a common convention, not a requirement.
- `pyre/generated/` is created by `pyre generate`.

CLI shortcut: `pyre docs project-structure`

## Project With Checked-In Migrations

```text
your-project/
├── pyre/
│   ├── schema.pyre
│   ├── query.pyre
│   ├── migrations/
│   │   └── 202501161139_initial/
│   │       ├── migration.sql
│   │       └── schema.diff
│   └── generated/
└── db/
    └── app.db
```

Use this layout when you want migration files in source control.

See [Migration Guide](./migrations.md).

CLI shortcut: `pyre docs migrations`

## Multi-Namespace Project

```text
your-project/
├── pyre/
│   ├── schema/
│   │   ├── App/
│   │   │   └── schema.pyre
│   │   └── Auth/
│   │       └── schema.pyre
│   ├── query.pyre
│   └── generated/
└── db/
    └── app.db
```

Use this when different schema partitions need separate namespaces.

See [Namespacing](./namespacing.md).

CLI shortcut: `pyre docs namespacing`
