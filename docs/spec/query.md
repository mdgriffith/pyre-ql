# Pyre Query Language Specification

## Overview

Pyre query files define database operations: `query` (select), `insert`, `update`, and `delete`. Query files use the `.pyre` extension and are typically named `query.pyre` or `queries.pyre`, or organized in a `queries/` directory.

## Syntax Rules

- **Indentation**: Query definitions must start at column 1 (beginning of line).
- **Comments**: Single-line comments using `//` are supported.
- **Whitespace**: Blank lines are allowed between queries.

## Query Operations

### Query (Select)

Selects data from records.

**Basic Syntax:**
```pyre
query QueryName {
    recordName {
        field1
        field2
    }
}
```

**With Parameters:**
```pyre
query GetUser($id: Int) {
    user {
        @where { id = $id }
        id
        name
    }
}
```

**With Nested Fields:**
```pyre
query GetPost($id: Int) {
    post {
        @where { id = $id }
        id
        title
        author {
            id
            name
            email
        }
    }
}
```

**Field Aliases:**
```pyre
query GetUsers {
    user {
        id
        username: name
        emailAddress: email
    }
}
```

**Multiple Root Fields:**
```pyre
query GetData {
    user {
        id
        name
    }
    post {
        id
        title
    }
}
```

### Insert

Inserts new records.

**Basic Syntax:**
```pyre
insert CreateUser($name: String, $email: String) {
    user {
        name = $name
        email = $email
    }
}
```

**With Session Variables:**
```pyre
insert CreatePost($title: String, $content: String) {
    post {
        authorUserId = Session.userId
        title = $title
        content = $content
        published = False
    }
}
```

**With Nested Inserts:**
```pyre
insert CreateUserWithPosts($name: String) {
    user {
        name = $name
        posts {
            title = "First Post"
            content = "Content here"
        }
    }
}
```

**With Union Type Values:**
```pyre
// Simple variant
insert CreateRecord($name: String) {
    record {
        name = $name
        status = Active
    }
}

// Variant with fields
insert CreateRecord($name: String, $reason: String) {
    record {
        name = $name
        status = Pending { reason = $reason }
    }
}
```

### Update

Updates existing records.

**Basic Syntax:**
```pyre
update UpdateUser($id: Int, $name: String?) {
    user {
        @where { id = $id }
        name = $name
    }
}
```

**Multiple Fields:**
```pyre
update UpdatePost($id: Int, $title: String?, $content: String?, $published: Bool?) {
    post {
        @where { id = $id }
        title = $title
        content = $content
        published = $published
    }
}
```

**Note**: Nullable parameters (`String?`) allow omitting fields in updates. Non-nullable parameters require values.

### Delete

Deletes records.

**Basic Syntax:**
```pyre
delete DeleteUser($id: Int) {
    user {
        @where { id = $id }
        id
    }
}
```

**Note**: Delete queries must include at least one field in the selection (typically `id`) for the return value.

## Parameters

Parameters are declared in the query signature:

```pyre
query QueryName($param1: Type, $param2: Type?) {
    // ...
}
```

**Supported Parameter Types:**
- `Int`
- `Float`
- `String`
- `Bool`
- `DateTime`
- `Date`
- Custom types (tagged unions)

**Nullable Parameters:**
Append `?` to make parameters optional:
```pyre
update UpdateUser($id: Int, $name: String?) {
    // $name can be omitted
}
```

## Field Selection

### Simple Fields

```pyre
id
name
email
createdAt
```

### Nested Fields (Links)

```pyre
author {
    id
    name
}
```

### Field Aliases

```pyre
username: name
emailAddress: email
myAuthor: author {
    id
    name
}
```

## Query Arguments

### @where

Filters records using conditions.

**Basic Syntax:**
```pyre
@where { field = value }
```

**With Variables:**
```pyre
@where { id = $id }
```

**With Session Variables:**
```pyre
@where { authorId = Session.userId }
```

**Multiple Conditions (AND):**
```pyre
@where { 
    published = True
    authorId = Session.userId
}
```

**OR Conditions:**
```pyre
@where { 
    status = "active" || status = "pending"
}
```

**Complex Conditions:**
```pyre
@where {
    (authorId = Session.userId || Session.role = "admin") &&
    published = True
}
```

**Note**: Multiple `@where` clauses are combined with AND. Use `||` within a single `@where` for OR conditions.

### @sort

Orders results.

**Ascending:**
```pyre
@sort name asc
```

**Descending:**
```pyre
@sort createdAt desc
```

**Multiple Sorts:**
```pyre
@sort createdAt desc
@sort name asc
```

### @limit

Limits the number of results.

```pyre
@limit 10
@limit $limitValue
```

## Where Clause Operators

**Comparison Operators:**
- `=` - Equal
- `!=` - Not equal
- `>` - Greater than
- `<` - Less than
- `>=` - Greater than or equal
- `<=` - Less than or equal
- `in` - In array (e.g., `id in [1, 2, 3]`)

**Logical Operators:**
- `&&` - AND
- `||` - OR

**Note**: Parentheses can be used for grouping: `(a = 1 || a = 2) && b = 3`

## Values

### Literals

**Strings:**
```pyre
name = "John"
title = "My Post"
```

**Integers:**
```pyre
count = 42
id = 1
```

**Floats:**
```pyre
price = 19.99
ratio = 0.5
```

**Booleans:**
```pyre
published = True
active = False
```

**Null:**
```pyre
name = Null
```

### Variables

**Query Parameters:**
```pyre
name = $name
id = $id
```

**Session Variables:**
```pyre
authorId = Session.userId
role = Session.role
```

### Type Values

**Simple Variants:**
```pyre
status = Active
action = Delete
```

**Variants with Fields:**
```pyre
status = Pending { 
    reason = $reason 
}

action = Create { 
    name = $name
    description = $description
}
```

**Note**: All fields of a variant must be provided when using variants with fields.

### Functions

SQLite functions can be used in values:

```pyre
// String functions
name = upper($name)
substring = substr($text, 0, 10)

// Math functions
maxValue = max($a, $b)
rounded = round($value)

// Date functions
dateStr = date("now")
```

**Common Functions:**
- String: `upper`, `lower`, `length`, `substr`, `trim`, `replace`
- Math: `max`, `min`, `abs`, `round`, `floor`, `ceil`
- Date: `date`, `time`, `datetime`, `strftime`

## Examples

### Complete Query File

```pyre
// Get a single user
query GetUser($id: Int) {
    user {
        @where { id = $id }
        id
        name
        email
        createdAt
    }
}

// List users with sorting
query ListUsers {
    user {
        @sort createdAt desc
        id
        name
        email
    }
}

// Get post with author
query GetPost($id: Int) {
    post {
        @where { id = $id }
        id
        title
        content
        published
        createdAt
        author {
            id
            name
            email
        }
    }
}

// Create user
insert CreateUser($name: String, $email: String) {
    user {
        name = $name
        email = $email
    }
}

// Create post with author from session
insert CreatePost($title: String, $content: String) {
    post {
        authorUserId = Session.userId
        title = $title
        content = $content
        published = False
    }
}

// Update post
update UpdatePost($id: Int, $title: String?, $content: String?) {
    post {
        @where { id = $id }
        title = $title
        content = $content
    }
}

// Delete post
delete DeletePost($id: Int) {
    post {
        @where { id = $id }
        id
    }
}

// Complex query with filters
query GetPublishedPosts($limit: Int) {
    post {
        @where { 
            published = True &&
            (authorId = Session.userId || Session.role = "admin")
        }
        @sort createdAt desc
        @limit $limit
        id
        title
        content
        author {
            id
            name
        }
    }
}
```

## Unexpected Behaviors

1. **Field selection in deletes**: Delete queries require at least one field selection (typically `id`) even though the record is being deleted.

2. **Multiple @where clauses**: Multiple `@where` directives are combined with AND, not OR. Use `||` within a single `@where` for OR conditions.

3. **Nullable parameters**: In updates, nullable parameters (`String?`) allow omitting fields. Non-nullable parameters must be provided.

4. **Nested inserts**: When inserting nested records, the parent record must exist or be created in the same operation. Foreign key constraints apply.

5. **Union variant fields**: When using union variants with fields, all fields must be provided. Partial field assignment is not supported.

6. **Session variable access**: Session variables are accessed via `Session.fieldName`, not `$Session.fieldName` or other syntax.

7. **Function arguments**: SQLite functions accept specific types. Type mismatches will cause errors at query execution time, not parse time.

8. **Sort order**: Multiple `@sort` directives are applied in order (first sort is primary, subsequent sorts are secondary, etc.).

9. **Limit placement**: `@limit` can appear anywhere in the field list, but typically appears after `@where` and `@sort`.

10. **Field aliases**: Aliases only affect the output structure, not the query logic. You cannot use aliases in `@where` clauses.

