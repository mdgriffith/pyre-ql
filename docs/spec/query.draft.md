# Pyre Query Language Drafts

This document captures draft or postponed features that are not part of the active spec.

## Schema-Derived Query Inputs (Draft)

Pyre can generate schema-derived input types and CRUD queries to reduce boilerplate while
keeping all operations explicit and type-safe.

### `{Table}.Conditions`

Represents allowed filter conditions for a table based on schema and permissions.

```text
Task.Conditions
  id: Task.IdCondition
  status: Task.StatusCondition
  createdAt: Task.CreatedAtCondition
  ...
```

**JSON format**

The JSON representation mirrors the `WhereArg` structure in the AST:

```json
{
  "status": "active",
  "$or": [
    { "priority": { "$gte": 3 } },
    { "assigneeId": { "$in": [1, 2, 3] } }
  ]
}
```

Session variables can be referenced as values using a `$session` object:

```json
{
  "assigneeId": { "$eq": { "$session": "userId" } },
  "role": { "$eq": { "$session": "role" } }
}
```

Supported operators:
- `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`
- `$in`, `$nin`
- `$like`, `$nlike`

Notes:
- A plain value (e.g. `"status": "active"`) is treated as `$eq`.
- `$and`/`$or` take a list of condition objects.
- `Null` means “no conditions”.
- Session variables can appear in value positions via `{ "$session": "fieldName" }`.

### `{Table}.OrderBy`

Represents allowed sort fields for a table.

```text
Task.OrderBy
  field: Task.SortableField
  direction: SortDirection
```

Notes:
- `{Table}.Options.orderBy` is always a list; use an empty list for “no sorting”.

### `{Table}.Includes`

Represents which relationships should be included by default queries.

```text
Task.Includes
  subtasks: Bool
  assignee: Bool
  project: Bool
```

### Example: Options in a Query

```pyre
query TaskGet($task: { where: Task.Conditions, orderBy: Task.OrderBy, limit: Int, include: Task.Include }) {
    task {
        @where($task.where)
        @sort($task.orderBy)
        @limit($task.limit)
        *
        subtasks @if($task.include.subtasks) {
            *
        }
        assignee @if($task.include.assignee) {
            *
        }
    }
}
```

### Auto-Generated CRUD Queries

For each table, Pyre can generate basic CRUD queries that use schema-derived inputs.
These queries are explicit and can be customized or overridden by user-defined queries.

**Select (List)**
```pyre
query TaskGet($options: Task.Options) {
    task {
        @where($options.where)
        @sort($options.orderBy)
        @limit($options.limit)
        *
        subtasks @if($options.include.subtasks) {
            *
        }
        assignee @if($options.include.assignee) {
            *
        }
    }
}
```

**Select (Single by ID)**
```pyre
query TaskGetById($id: Task.id) {
    task {
        @where { id == $id }
        *
    }
}
```

**Insert**
```pyre
insert TaskCreate($input: Task.CreateInput) {
    task {
        @set($input)
    }
}
```

**Update**
```pyre
update TaskUpdate($id: Task.id, $input: Task.UpdateInput) {
    task {
        @where { id == $id }
        @set($input)
    }
}
```

**Delete**
```pyre
delete TaskDelete($id: Task.id) {
    task {
        @where { id == $id }
        id
    }
}
```

**Notes**
- `CreateInput` and `UpdateInput` are derived from schema + permissions.
- The auto-generated queries are meant as defaults; explicit queries can override them.
- `CreateInput` requires all non-nullable fields without defaults.
- `UpdateInput` fields are optional; omitted fields are unchanged and explicit `Null` clears the field.
- Auto-generated `Get` queries include relationships only when their `Options.include` flag is true.

## Why This Is Drafted

We explored `{Table}.Conditions` as a JSON-driven `@where` input, but the static SQL approach leads to very large predicates:
- Each column expands into a full operator matrix (`$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`, `$like`, `$nlike`).
- `$and`/`$or` require recursive expansion, which multiplies the predicate size.
- SQLite can hit parser stack limits for larger schemas or deeper nesting.

## Approaches If Revisited

1) Runtime JSON -> AST -> SQL
   - Parse the JSON conditions at runtime and generate a tight WHERE clause with only the needed fields/operators.
   - Keeps SQL small and fast but moves some SQL generation to runtime.

2) Generic JSON-eval SQL
   - Keep SQL static, but evaluate conditions by iterating `json_each` and interpreting operators with `CASE` logic.
   - Preserves static SQL but increases runtime work and complexity in SQL.

3) Helper/UDF or Recursive CTE
   - Use a user-defined function or recursive CTE to interpret conditions without inlining the whole predicate.
   - Most complex, but can keep SQL small and support deep nesting.
