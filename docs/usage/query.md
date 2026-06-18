# Pyre Query Guide

Pyre query files define typed database operations against a Pyre schema. Query files usually live in the same `pyre/` tree as schema files. Any non-schema `.pyre` file under that tree is treated as a query file. A common convention is `pyre/query.pyre`.

## Select Queries

Use `query` to read records and shape the returned data.

```pyre
query GetUser($id: Int) {
    user {
        @where { id == $id }
        id
        name
    }
}
```

Nested selections follow schema links.

```pyre
query GetPosts {
    post {
        id
        title
        author {
            id
            name
        }
    }
}
```

## Mutations

Use `insert`, `update`, and `delete` for writes.

```pyre
insert CreateUser($name: String) {
    user {
        name = $name
    }
}
```

```pyre
update RenameUser($id: Int, $name: String) {
    user {
        @where { id == $id }
        name = $name
    }
}
```

```pyre
delete DeleteUser($id: Int) {
    user {
        @where { id == $id }
    }
}
```

## Parameters And Filters

Declare parameters in the operation signature and reference them with `$name`.

```pyre
query SearchUsers($name: String) {
    user {
        @where { name == $name }
        id
        name
    }
}
```

Session values can also participate in query conditions:

```pyre
query MyNotes {
    note {
        @where { ownerId == Session.userId }
        id
        body
    }
}
```

## Generated CRUD

Pyre can expose schema-derived CRUD mutations for writable tables. Use handwritten queries when you need custom filters, nested writes, business rules, or a response shape that differs from the default generated operation.

## Validation Flow

Use:

```bash
pyre check
```

after editing query files.

MCP note:

- use `pyre_preview_query` to typecheck dynamic query text and inspect generated SQL
- use `pyre_explain_query` to validate params/session and inspect a real query plan
- use `pyre_query` to validate and execute dynamic query text without creating a query file
