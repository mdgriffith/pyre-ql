# Pyre Schema Guide

Pyre schema files define the database shape that Pyre typechecks, migrates, and uses to generate query APIs. A single-schema project usually stores this in `pyre/schema.pyre`; namespaced projects use `pyre/schema/<Namespace>/schema.pyre`.

## Records

Records become database tables.

```pyre
record User {
    id   Int    @id
    name String
    @public
}
```

Common scalar types are `Int`, `Float`, `String`, `Bool`, `DateTime`, `Date`, and `JSON`. Add `?` for nullable fields, for example `deletedAt DateTime?`.

## Links

Links describe relationships between records.

```pyre
record Post {
    id       Int @id
    authorId Int
    author   @link(authorId, User.id)
    @public
}
```

In namespaced schemas, cross-namespace links use `Namespace.Record.field`.

```pyre
author @link(authorId, Auth.User.id)
```

## Directives

Use directives to describe table behavior and constraints.

```pyre
record Membership {
    id        Int @id
    orgId     Int
    userId    Int
    deletedAt DateTime?

    @unique(orgId, userId)
    @index(orgId asc) where { deletedAt = null }
    @public
}
```

Useful directives include `@id`, `@default(...)`, `@unique(...)`, `@index(...)`, `@public`, permission directives, `@timestamps`, and `@syncable(false)`.

## Types

Use `type` declarations for tagged unions and reusable domain values.

```pyre
type Status
   = Active
   | Inactive
   | Blocked { reason String }
```

## Sessions

Session definitions describe values supplied by the application runtime, often for authorization-aware queries.

```pyre
session {
    userId Int
}
```

## Validation Flow

After writing schema source, run `pyre_check`. For new projects through MCP, prefer `pyre_init` with a required `schema` argument so the schema is validated before files are created.
