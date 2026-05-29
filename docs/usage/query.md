# Pyre Query Guide

Pyre query files define typed database operations against a Pyre schema. Query files usually live in the same `pyre/` tree as schema files and are validated with `pyre_check`.

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

## Generated CRUD

Pyre can expose schema-derived CRUD mutations for writable tables. Use handwritten queries when you need custom filters, nested writes, business rules, or a response shape that differs from the default generated operation.

## Validation Flow

Use `pyre_check` after editing query files. To test an ad hoc query through MCP without creating a query file, use `pyre_query` with `database`, `query`, and optional `params`.
