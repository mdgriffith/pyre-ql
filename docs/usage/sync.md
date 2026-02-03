# Sync Setup (Client/Server)

This setup is for apps that run a server and want a typesafe client that calls it. Pyre generates:

- **Server code** to execute queries and enforce permissions
- **Client code** to call the server with typed inputs/outputs

## 1. Define schema and queries

Create `pyre/schema.pyre` and `pyre/queries.pyre`:

```pyre
session {
    userId Int
}

record User {
    @public
    id        Int     @id
    name      String
    email     String
    createdAt DateTime @default(now)
}

query GetUser($id: Int) {
    user {
        @where { id == $id }
        id
        name
        email
    }
}
```

## 2. Migrate the database

```bash
touch db/app.db
pyre migrate db/app.db
```

## 3. Generate code

```bash
pyre generate
```

Generated output (default):

```
pyre/generated/
├── client/
│   ├── node/
│   └── elm/
└── server/
    └── typescript/
```

## 4. Server usage (TypeScript)

```typescript
import * as Query from "./pyre/generated/server/typescript/query";

const env = {
  url: "file:./db/app.db",
  authToken: undefined,
};

const session = { userId: 1 };

const result = await Query.run(env, "GetUser", session, { id: 1 });
if (result.kind === "success") {
  console.log(result.data);
} else {
  console.error(result.message);
}
```

## 5. Client usage (TypeScript)

```typescript
import { request } from "./pyre/generated/client/node/query";

const response = await request("GetUser", { id: 1 });
console.log(response);
```

## Notes

- Use this setup when you want a networked client/server architecture.
- Permissions are enforced on the server using session data.
- The client is just a thin, typed wrapper around your HTTP API.
