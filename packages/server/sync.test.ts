// @ts-nocheck
import { afterEach, expect, mock, test } from "bun:test";

const defaultSyncSql = () => ({
  tables: [
    {
      table_name: "maps",
      permission_hash: "perm",
      sql: ["select 1"],
      headers: [
        "id",
        "name",
        "tiling",
        "tiling__tileRootKey",
        "tiling__tileWidth",
        "tiling__format",
        "updatedAt",
      ],
      json_columns: [],
    },
  ],
});

const defaultReshapeSyncTableGroups = () => ([
  {
    table_name: "maps",
    headers: ["id", "name", "tiling", "updatedAt"],
    rows: [
      [
        1,
        "World",
        {
          type: "Tiling",
          tileRootKey: "tiles/root",
          tileWidth: 256,
          format: { type: "Png" },
        },
        1700000000,
      ],
    ],
  },
]);

let getSyncSqlMock = defaultSyncSql;
let reshapeSyncTableGroupsMock = defaultReshapeSyncTableGroups;
let introspectionResult = { schema_source: "" };
let setSchemaCalls: unknown[] = [];

mock.module("./wasm/pyre_wasm.js", () => ({
  sql_is_initialized: () => "select 1 as is_initialized",
  sql_introspect: () => "select introspection",
  get_sync_status_sql: () => "select 1",
  get_sync_sql: () => getSyncSqlMock(),
  calculate_sync_deltas: () => ({ groups: [] }),
  reshape_sync_table_groups: (groups: any) => reshapeSyncTableGroupsMock(groups),
  set_schema: (introspection: unknown) => setSchemaCalls.push(introspection),
}));

const { catchup } = await import("./sync");
const { loadSchemaFromDatabase } = await import("./schema");

afterEach(() => {
  getSyncSqlMock = defaultSyncSql;
  reshapeSyncTableGroupsMock = defaultReshapeSyncTableGroups;
  introspectionResult = { schema_source: "" };
  setSchemaCalls = [];
});

test("catchup activates the schema loaded for its databaseId", async () => {
  getSyncSqlMock = () => ({ tables: [] });
  const mainIntrospection = { schema_source: "main schema" };
  const campaignIntrospection = { schema_source: "campaign schema" };
  const schemaDb = {
    execute: mock(async (sql: string) => {
      if (sql.includes("is_initialized")) {
        return { rows: [{ is_initialized: 1 }] };
      }

      return { rows: [{ result: JSON.stringify(introspectionResult) }] };
    }),
  };
  const db = {
    execute: mock(async () => ({ rows: [] })),
    batch: mock(async () => ([])),
  };

  introspectionResult = mainIntrospection;
  await loadSchemaFromDatabase("main", schemaDb as any);
  introspectionResult = campaignIntrospection;
  await loadSchemaFromDatabase("campaign", schemaDb as any);

  await catchup(db as any, { tables: {} }, {}, 1000, "main");

  expect(setSchemaCalls.at(-1)).toEqual(mainIntrospection);
});

test("catchup reshapes flattened custom types before returning sync rows", async () => {
  const db = {
    execute: mock(async () => ({ rows: [{ table_name: "maps", needs_sync: 1 }] })),
    batch: mock(async () => ([
      {
        columns: [
          "id",
          "name",
          "tiling",
          "tiling__tileRootKey",
          "tiling__tileWidth",
          "tiling__format",
          "updatedAt",
        ],
        rows: [
          {
            id: 1,
            name: "World",
            tiling: "Tiling",
            tiling__tileRootKey: "tiles/root",
            tiling__tileWidth: 256,
            tiling__format: "Png",
            updatedAt: 1700000000n,
          },
        ],
      },
    ])),
  };

  const result = await catchup(db as any, { tables: {} }, {}, 1000);

  expect(result).toEqual({
    tables: {
      maps: {
        rows: [
          {
            id: 1,
            name: "World",
            tiling: {
              type: "Tiling",
              tileRootKey: "tiles/root",
              tileWidth: 256,
              format: { type: "Png" },
            },
            updatedAt: 1700000000,
          },
        ],
        permission_hash: "perm",
        last_seen_updated_at: 1700000000,
      },
    },
    has_more: false,
  });
});

test("catchup stamps response with databaseId when provided", async () => {
  getSyncSqlMock = () => ({ tables: [] });
  const db = {
    execute: mock(async () => ({ rows: [] })),
    batch: mock(async () => ([])),
  };

  const result = await catchup(db as any, { tables: {} }, {}, 1000, "campaign:123");

  expect(result.databaseId).toBe("campaign:123");
});

test("catchup normalizes bigint row values before reshaping", async () => {
  const db = {
    execute: mock(async () => ({ rows: [{ table_name: "maps", needs_sync: 1 }] })),
    batch: mock(async () => ([
      {
        columns: [
          "id",
          "name",
          "tiling",
          "tiling__tileRootKey",
          "tiling__tileWidth",
          "tiling__format",
          "updatedAt",
        ],
        rows: [
          {
            id: 1n,
            name: "World",
            tiling: "Tiling",
            tiling__tileRootKey: "tiles/root",
            tiling__tileWidth: 256n,
            tiling__format: "Png",
            updatedAt: 1700000000,
          },
        ],
      },
    ])),
  };

  const result = await catchup(db as any, { tables: {} }, {}, 1000);

  expect(result.tables.maps.rows[0]).toEqual({
    id: 1,
    name: "World",
    tiling: {
      type: "Tiling",
      tileRootKey: "tiles/root",
      tileWidth: 256,
      format: { type: "Png" },
    },
    updatedAt: 1700000000,
  });
  expect(result.tables.maps.last_seen_updated_at).toBe(1700000000);
});

test("catchup unwraps double-encoded json objects for json columns", async () => {
  getSyncSqlMock = () => ({
    tables: [
      {
        table_name: "gameEntities",
        permission_hash: "perm",
        sql: ["select 1"],
        headers: ["id", "attrs", "updatedAt"],
        json_columns: ["attrs"],
      },
    ],
  });
  reshapeSyncTableGroupsMock = (groups: any) => groups;

  const db = {
    execute: mock(async () => ({ rows: [{ table_name: "gameEntities", needs_sync: 1 }] })),
    batch: mock(async () => ([
      {
        columns: ["id", "attrs", "updatedAt"],
        rows: [
          {
            id: 1,
            attrs: '"{\\"position\\":{\\"x\\":11,\\"y\\":14}}"',
            updatedAt: 1700000000,
          },
        ],
      },
    ])),
  };

  const result = await catchup(db as any, { tables: {} }, {}, 1000);

  expect(result.tables.gameEntities.rows[0]).toEqual({
    id: 1,
    attrs: {
      position: {
        x: 11,
        y: 14,
      },
    },
    updatedAt: 1700000000,
  });
});
