# Getting Started with Pyre

Pyre is a schema and query language for building typesafe persistence using SQLite. This guide will walk you through setting up and using Pyre in your project.

## Overview

Pyre supports two setups depending on how you want to run queries:

1. **Sync (client/server)** - Generate server query handlers and a typesafe client that calls them.
2. **Simple (direct SQLite)** - Generate standalone TypeScript functions that execute SQL directly against SQLite.

Choose one of the following guides:

- [Sync setup](./sync.md)
- [Simple setup](./simple.md)

Both setups use the same schema/query language, but differ in how you execute queries.

## Step 1: Create a Schema

Create a `pyre/` directory in your project root and add a `schema.pyre` file:

```pyre
record User {
    accounts @link(Account.userId)
    posts    @link(Post.authorUserId)

    id        Int     @id
    name      String?
    status    Status
    createdAt DateTime @default(now)
}

record Account {
    id     Int   @id
    userId Int
    name   String
    status String
    users  @link(userId, User.id)
}

record Post {
    id           Int     @id
    createdAt    DateTime @default(now)
    authorUserId Int
    title        String
    content      String
    status       Status
    users        @link(authorUserId, User.id)
}

type Status
   = Active
   | Inactive
   | Special {
        reason String
     }
```

This schema defines:
- **Records** - Your database tables (User, Account, Post)
- **Fields** - Columns with types (Int, String, DateTime, etc.)
- **Links** - Relationships between records
- **Types** - Custom types like enums (Status)

## Step 2: Apply Migrations

Create your database file and run migrations:

```bash
# Create the database file
touch db/playground.db

# Run migrations
pyre migrate db/playground.db
```

This will:
- Compare your schema to the current database state
- Generate migration files in `pyre/migrations/`
- Apply the migrations to your database

The migration files include:
- `migration.sql` - The SQL to execute
- `schema.diff` - The schema changes

## Step 3: Write Queries

Create a `query.pyre` file in your `pyre/` directory:

```pyre
// Query a user by ID
query MyQuery($id: Int) {
    user {
        @where { id = $id }
        id
        createdAt
        username: name
        myAccounts: accounts {
            id
            name
            status
        }
    }
}

// Insert a new user
insert UserNew($name: String, $status: Status) {
    user {
        name = $name
        status = $status
        accounts {
            name = "My account"
            status = "Untyped status"
        }
        posts {
            title = "My first post"
            content = "This is my first post"
        }
    }
}

// Update posts
update UpdateBlogPosts($userId: Int, $status: Status) {
    post {
        @where { authorUserId = $userId }
        title = "My First Post"
        content = "This is my first post"
        status = $status
    }
}

// Delete an account
delete RemoveAccount($accountId: Int) {
    account {
        @where { id = $accountId }
        id
    }
}
```

## Step 4: Generate Code

Generate type-safe client and server code:

```bash
pyre generate
```

This creates generated code in `pyre/generated/`:
- **TypeScript core** - Shared types/decoders + SQL + metadata
  - `typescript/core/`
- **TypeScript targets** - Thin adapters for each use case
  - `typescript/server.ts`
  - `typescript/run.ts`
- **Elm client**
  - `client/elm/`

## Step 5: Use in Your Application

### Server Example (TypeScript/Node.js)

```typescript
import * as Query from "./pyre/generated/typescript/server";

const env = {
    url: "file:./db/playground.db",
    authToken: undefined,
};

const session = {};

// Run a query
const result = await Query.run(env, "MyQuery", session, { id: 1 });

if (result.kind === "success") {
    console.log(result.data);
} else {
    console.error(result.message);
}
```

### Client Example (TypeScript)

```typescript
import { meta as MyQuery } from "./pyre/generated/typescript/core/queries/metadata/myQuery";

// Use the generated query metadata with your client runtime
console.log(MyQuery.id);
```

## Additional Commands

### Type Checking

Check your schema and queries for errors:

```bash
pyre check
```

### Formatting

Format your Pyre files:

```bash
pyre format
```

### Introspection

Generate a schema from an existing database:

```bash
pyre introspect db/playground.db
```

### Initialize a New Project

Generate a starter schema:

```bash
pyre init
```

For multi-database setups:

```bash
pyre init --multidb
```

## Project Structure

A typical Pyre project structure looks like:

```
your-project/
├── pyre/
│   ├── schema.pyre          # Your schema definitions
│   ├── query.pyre            # Your queries
│   ├── migrations/          # Generated migration files
│   │   └── 202501161139_init/
│   │       ├── migration.sql
│   │       └── schema.diff
│   └── generated/           # Generated code
│       ├── client/
│       └── server/
└── db/
    └── playground.db        # Your SQLite database
```

## Next Steps

- Explore the [playground examples](../playground/) for more complex usage
- Check out the CLI help: `pyre --help`
- Read about advanced features in the schema and query syntax
