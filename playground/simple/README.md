# Simple Pyre Playground

This playground demonstrates using Pyre as a direct SQLite interface without client/server sync. Perfect for:
- Scripts and CLI tools
- Single-user applications
- Local-first apps without sync needs
- Server-side usage where you just want typesafe DB operations

## Features

- **Typesafe operations**: Each query generates a typed async function
- **No server required**: Direct SQLite access via libsql client
- **Session support**: Permission-based access control built-in
- **Full type safety**: Input types, return types, and session types are all generated

## Structure

```
playground/simple/
├── pyre/
│   ├── schema.pyre          # Your data model
│   ├── queries.pyre         # Your operations
│   └── generated/           # Generated TypeScript code
│       └── simple/
│           ├── types.ts     # Session, User, Post types
│           ├── db.ts        # Database setup
│           ├── index.ts     # Export all queries
│           └── queries/     # Individual query functions
│               ├── getUser.ts
│               ├── createUser.ts
│               └── ...
├── src/
│   └── index.ts            # Demo usage
└── db/
    └── app.db              # SQLite database file
```

## Usage

### 1. Define your schema (`pyre/schema.pyre`)

```pyre
session {
    userId Int
    role String
}

record User {
    @public
    id        Int     @id
    name      String
    email     String
    createdAt DateTime @default(now)
}

record Post {
    @allow(query) { authorUserId == Session.userId || published == true }
    @allow(insert) { authorUserId == Session.userId }
    id           Int     @id
    authorUserId Int
    title        String
    content      String
    published    Bool   @default(false)
    createdAt    DateTime @default(now)
}
```

### 2. Define your queries (`pyre/queries.pyre`)

```pyre
query GetUser($id: Int) {
    user {
        @where { id == $id }
        id
        name
        email
    }
}

insert CreateUser($name: String, $email: String) {
    user {
        name = $name
        email = $email
    }
}
```

### 3. Generate code

```bash
cd playground/simple
pyre generate --out pyre/generated
```

### 4. Use in your TypeScript code

```typescript
import { createClient } from '@libsql/client';
import { GetUser, CreateUser, Session } from './pyre/generated/typescript/run';

// Create database connection
const db = createClient({
  url: 'file:./db/app.db'
});

// Define session (used for permissions)
const session: Session = {
  userId: 1,
  role: 'admin'
};

// Create a user
await CreateUser(db, { 
  name: 'Alice', 
  email: 'alice@example.com' 
}, session);

// Query users
const result = await GetUser(db, { id: 1 }, session);
console.log(result.user[0].name); // 'Alice'
```

## Running the Demo

```bash
# Install dependencies
bun install

# Run the demo
bun run src/index.ts
```

## Generated Code

For each query, Pyre generates:

1. **Input type**: Typed parameters for the query
2. **Return type**: Typed result structure
3. **Session type**: Typed session for permission checking
4. **Async function**: Complete function that executes SQL and decodes results

Example generated function:
```typescript
export interface GetUserInput {
  id: number;
}

export interface GetUserResult {
  user: User[];
}

export async function GetUser(
  db: Client,
  input: GetUserInput,
  session: Session
): Promise<GetUserResult> {
  // SQL is generated at compile time
  const results = await db.batch([{
    sql: `...generated SQL...`,
    args: { id: input.id, session_userId: session.userId }
  }]);
  
  // Results are decoded into typed structure
  return decodeGetUserResult(results);
}
```

## Advantages

- **Zero runtime overhead**: SQL is generated at compile time
- **Full type safety**: Catch errors at compile time, not runtime
- **No ORM complexity**: Write SQL naturally with full control
- **Permission aware**: Session-based access control is built-in
- **No server needed**: Direct database access for simple use cases
