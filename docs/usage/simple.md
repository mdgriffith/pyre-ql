# Simple Setup (Direct SQLite)

This setup is for apps that want to execute queries directly against SQLite without a server. Pyre generates:

- **Standalone TypeScript functions** for each query
- **Typed inputs/outputs** using ArkType decoding
- **Direct SQLite execution** via `@libsql/client`

## 1. Define schema and queries

Create `pyre/schema.pyre` and `pyre/queries.pyre`:

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
pyre generate --out pyre/generated
```

Generated output (simple target):

```
pyre/generated/typescript/targets/run/
└── run.ts
```

## 4. Use the generated functions

```typescript
import { createClient } from "@libsql/client";
import { GetUser, type Session } from "./pyre/generated/typescript/targets/run/run";

const db = createClient({ url: "file:./db/app.db" });
const session: Session = { userId: 1, role: "admin" };

const result = await GetUser(db, session, { id: 1 });
console.log(result.user[0]);
```

## Notes

- Use this setup for scripts, CLI tools, local apps, or server-side direct DB access.
- Permissions still apply (session is passed to each query).
- Return data is decoded via ArkType, so you get runtime validation as well as TypeScript types.
