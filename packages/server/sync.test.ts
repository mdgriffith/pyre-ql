// @ts-nocheck
import { expect, mock, test } from "bun:test";

mock.module("./wasm/pyre_wasm.js", () => ({
  get_sync_status_sql: () => "select 1",
  get_sync_sql: () => ({
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
  }),
  reshape_sync_table_groups: () => ([
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
  ]),
}));

const { catchup } = await import("./sync");

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
            updatedAt: 1700000000,
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
