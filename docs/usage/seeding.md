# Seeding Data

Pyre provides a server-side seed helper for creating fixture or import data without writing one-off mutation queries.

Seed data is shaped like the database schema. Top-level keys are table names, and nested keys are links declared in the Pyre schema.

```ts
import { createClient } from "@libsql/client";
import { seed } from "./pyre/generated/typescript/server";

const db = createClient({ url: "file:test.db" });

const result = await seed(db, {
  users: [
    {
      name: "Fred",
      posts: [
        { title: "example post", content: "My content!" },
        { title: "example post2", content: "My content!" },
      ],
    },
  ],
});
```

## How It Works

`seed` inserts each row and uses schema link metadata to connect nested records.

For example, if `users.posts` links `users.id` to `posts.authorId`, Pyre will:

1. Insert the user.
2. Read the inserted user's `id` from `RETURNING *`.
3. Insert each nested post with `authorId` set to that user id.
4. Return the inserted user and nested posts.

The returned shape follows the input shape and includes full inserted rows:

```ts
{
  kind: "success",
  response: {
    users: [
      {
        id: 1,
        name: "Fred",
        posts: [
          { id: 1, authorId: 1, title: "example post", content: "My content!" },
        ],
      },
    ],
  },
}
```

## Flat Foreign Keys

You can also seed in layers by setting foreign key columns directly:

```ts
await seed(db, {
  users: [{ id: 1, name: "Fred" }],
});

await seed(db, {
  posts: [{ authorId: 1, title: "example post", content: "My content!" }],
});
```

This is useful when importing existing data or when deterministic ids are convenient.

## JSON And Custom Types

Seed input should use the normal serialized JavaScript shape for JSON and custom type fields. Constructed Pyre values use an `_type` discriminator. Pyre serializes or flattens the value for SQLite internally.

For a JSON column:

```pyre
record Game {
    id    Id.Int @id
    state Json<GameState>
}
```

Provide the JSON value directly:

```ts
await seed(db, {
  games: [
    {
      name: "Token game",
      state: {
        _type: "GameState",
        groups: [],
        clocks: [],
      },
    },
  ],
});
```

Do not pre-stringify JSON values. Pyre handles that before insert.

For a custom type stored across multiple SQLite columns:

```pyre
record MapEntity {
    id        Id.Int @id
    placement MapEntityPlacement
}

type MapEntityPlacement
   = MapEntityGridPlacement { x Int, y Int }
   | MapEntityWorldPlacement { x Int, y Int, scale Int }
```

Provide the constructed value:

```ts
await seed(db, {
  mapEntities: [
    {
      placement: {
        _type: "MapEntityWorldPlacement",
        x: 10,
        y: 20,
        scale: 100,
      },
    },
  ],
});
```

Pyre writes the discriminator and flattened backing columns behind the scenes, and reconstructs the returned row into the constructed shape.

## Constraints

- `seed` is server-side only.
- Top-level keys must be table names, not record names.
- Nested keys must be links declared on the parent table.
- If a nested row provides a foreign key that conflicts with the value derived from the parent link, `seed` fails.
- The whole seed call is atomic. Pyre starts a transaction, commits on success, and rolls back on validation or insert failure.
- Inserts are sequential for now: Pyre inserts one row at a time so it can read generated parent ids and report path-specific errors.
- `seed` bypasses Pyre query permissions.
- `seed` does not currently update Pyre sync metadata. Use it for setup/import workflows before synced clients rely on live deltas.

## Runtime API

Generated server output exposes the schema-bound helper:

```ts
import { seed } from "./pyre/generated/typescript/server";

await seed(db, data);
```

The lower-level runtime helper is also available if you need to pass schema metadata explicitly:

```ts
import { seed } from "@pyre/server/query";
import { schemaMetadata } from "./pyre/generated/typescript/core/schema";

await seed(db, schemaMetadata, data);
```
