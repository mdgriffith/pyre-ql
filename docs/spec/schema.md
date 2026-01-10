# Pyre Schema Language Specification

## Overview

Pyre schema files define database structure using records (tables), types (tagged unions), and sessions. Schema files use the `.pyre` extension and are typically named `schema.pyre` or organized in a `schema/` directory.

## Syntax Rules

- **Indentation**: Top-level definitions (`record`, `type`, `session`) must start at column 1 (beginning of line). Indentation is not allowed.
- **Comments**: Single-line comments using `//` are supported.
- **Whitespace**: Blank lines are allowed between definitions.

## Records

Records define database tables. Each record maps to a SQLite table.

### Basic Syntax

```pyre
record RecordName {
    fieldName Type
    fieldName Type?
}
```

### Field Types

**Primitive Types:**
- `Int` - Integer (stored as INTEGER in SQLite)
- `Float` - Floating point number (stored as REAL)
- `String` - Text (stored as TEXT)
- `Bool` - Boolean (stored as INTEGER, 0 or 1)
- `DateTime` - Timestamp (stored as INTEGER, Unix epoch)
- `Date` - Date (stored as TEXT)
- `JSON` - JSON data (stored as BLOB)

**Nullable Types:**
Append `?` to make a field nullable:
```pyre
name String?
email String?
```

**Custom Types:**
Reference tagged union types defined elsewhere:
```pyre
status Status
action Action
```

### Column Directives

Directives modify column behavior:

**`@id`** - Primary key
```pyre
id Int @id
```

**`@unique`** - Unique constraint
```pyre
email String @unique
```

**`@index`** - Create an index
```pyre
createdAt DateTime @index
```

**`@default(value)`** - Default value
```pyre
// String literal
name String @default("Unknown")

// Numeric literal
count Int @default(0)

// Boolean literal
published Bool @default(False)

// DateTime now
createdAt DateTime @default(now)
```

**Note**: Multiple directives can be applied to a single field:
```pyre
createdAt DateTime @default(now) @index
```

### Record-Level Directives

These directives apply to the entire record:

**`@tablename("name")`** - Override default table name
```pyre
record User {
    @tablename("users")
    id Int @id
    name String
}
```

By default, table names are the pluralized, decapitalized record name (e.g., `User` → `users`).

**`@public`** - Make all operations public (no permission checks)
```pyre
record Post {
    @public
    id Int @id
    title String
}
```

**`@allow(operations) { conditions }`** - Permission rules
```pyre
record Post {
    // Single operation
    @allow(query) { published == True }
    
    // Multiple operations
    @allow(insert, update) { authorId == Session.userId }
    
    // All operations
    @allow(*) { authorId == Session.userId }
    
    // Complex conditions
    @allow(delete) { 
        authorId == Session.userId || Session.role == "admin" 
    }
}
```

**Operations**: `query`, `insert`, `update`, `delete`, or `*` for all.

**Conditions**: Use `Session.fieldName` to reference session variables. Supported operators: `==` (equal), `&&` (and), `||` (or).

**`@watch`** - Enable change watching for inserts
```pyre
record Post {
    @watch
    id Int @id
    title String
}
```

### Links

Links define relationships between records. They appear as field-level directives.

**Syntax:**
```pyre
record Post {
    authorId Int
    author @link(authorId, User.id)
}
```

Both the local field name and the foreign table's primary key must be explicitly specified.

**Multi-field links** (for composite keys):
```pyre
record Post {
    authorId Int
    authorType String
    author @link(authorId, authorType, User.id, User.type)
}
```

**Note**: Links are bidirectional. Defining a link on one record automatically creates a reverse link on the referenced record.

## Types (Tagged Unions)

Types define custom union types (sum types) similar to Rust enums or TypeScript discriminated unions.

### Basic Syntax

```pyre
type TypeName
   = Variant1
   | Variant2
   | Variant3
```

**Note**: The `=` and `|` separators can be on separate lines. Leading whitespace before variants is allowed.

### Variants with Fields

Variants can contain fields:

```pyre
type Status
   = Active
   | Inactive
   | Pending {
        reason String
        createdAt DateTime
    }
```

### Multiple Fields

```pyre
type Action
   = Create {
        name String
        description String
    }
   | Update {
        id Int
        changes String
    }
   | Delete {
        id Int
        reason String
    }
```

**Note**: Variants without fields are simple tags. Variants with fields use `{ }` syntax.

## Session

Sessions define runtime context variables available in queries and permissions.

### Syntax

```pyre
session {
    userId Int
    role String
    organizationId Int?
}
```

Session fields can be nullable using `?`.

**Usage in permissions:**
```pyre
@allow(query) { userId = Session.userId }
@allow(delete) { role = Session.role || Session.role = "admin" }
```

**Usage in queries:**
```pyre
insert CreatePost($title: String) {
    post {
        authorId = Session.userId
        title = $title
    }
}
```

## Examples

### Complete Schema

```pyre
session {
    userId Int
    role String
}

type Status
   = Active
   | Inactive
   | Pending {
        reason String
    }

record User {
    @tablename("users")
    @public
    
    id Int @id
    name String?
    email String? @unique
    status Status
    createdAt DateTime @default(now)
    
    posts @link(Post.authorUserId)
    accounts @link(Account.userId)
}

record Post {
    @watch
    @allow(query) { published = True }
    @allow(insert, update, delete) { authorUserId = Session.userId }
    
    id Int @id
    createdAt DateTime @default(now)
    authorUserId Int
    title String
    content String
    published Bool @default(False)
    
    users @link(authorUserId, User.id)
}
```

## Unexpected Behaviors

1. **Table name pluralization**: Record names are automatically pluralized and decapitalized for table names. `User` becomes `users`, `Person` becomes `people` (if pluralization rules apply).

2. **Link direction**: Links are always bidirectional. Defining `@link` on one record creates a reverse link automatically.

3. **Indentation strictness**: Top-level definitions cannot be indented. They must start at column 1.

4. **Nullable syntax**: The `?` must come immediately after the type name, before any directives: `name String? @unique` not `name String @unique?`.

5. **Default values**: Only specific literal types are supported in `@default()`. Complex expressions are not allowed.

6. **Permission conditions**: Only `Session.*` variables can be referenced in permission conditions. Record fields cannot be compared directly in permissions.

7. **Type variants**: Variants with fields require all fields to be provided when used in queries. There's no partial field support.

8. **Comments**: Only single-line `//` comments are supported. Multi-line comments are not available.

