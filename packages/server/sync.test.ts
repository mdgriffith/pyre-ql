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

mock.module("./wasm/pyre_wasm.js", () => ({
  get_sync_status_sql: () => "select 1",
  get_sync_sql: () => getSyncSqlMock(),
  reshape_sync_table_groups: (groups: any) => reshapeSyncTableGroupsMock(groups),
}));

const { catchup } = await import("./sync");

afterEach(() => {
  getSyncSqlMock = defaultSyncSql;
  reshapeSyncTableGroupsMock = defaultReshapeSyncTableGroups;
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
