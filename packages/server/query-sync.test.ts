// @ts-nocheck
import { beforeEach, expect, mock, test } from "bun:test";

let introspectionResult = { schema_source: "test schema" };
let sessionIds = ["s1"];
let reshapedRows = [[1, "World", { type: "Tiling", tileRootKey: "tiles/root", tileWidth: 256, format: { type: "Png" } }]];

mock.module("./wasm/pyre_wasm.js", () => ({
  sql_is_initialized: () => "select 1 as is_initialized",
  sql_introspect: () => "select introspection",
  set_schema: () => undefined,
  get_sync_status_sql: () => "select 1",
  get_sync_sql: () => ({ tables: [] }),
  calculate_sync_deltas: () => ({
    groups: [
      {
        session_ids: sessionIds,
        table_groups: [
          {
            table_name: "maps",
            headers: [
              "id",
              "name",
              "tiling",
              "tiling__tileRootKey",
              "tiling__tileWidth",
              "tiling__format",
            ],
            rows: [[1, "World", "Tiling", "tiles/root", 256, "Png"]],
          },
        ],
      },
    ],
  }),
  reshape_sync_table_groups: () => ([
    {
      table_name: "maps",
      headers: ["id", "name", "tiling"],
      rows: reshapedRows,
    },
  ]),
}));

mock.module("./query", () => ({
  run: async (
    _db: unknown,
    _queryMap: unknown,
    _queryId: string,
    _args: unknown,
    _executingSession: unknown,
    connectedSessions: Map<string, { session: Record<string, unknown> }>,
    syncDeltas: (affectedRowGroups: unknown[], connectedSessions: Map<string, { session: Record<string, unknown> }>, sendToSession: (sessionId: string, message: unknown) => void) => Promise<void>,
  ) => ({
    kind: "success",
    response: {},
    sync: async (sendToSession: (sessionId: string, message: unknown) => void) => {
      await syncDeltas(
        [
          {
            table_name: "maps",
            headers: [
              "id",
              "name",
              "tiling",
              "tiling__tileRootKey",
              "tiling__tileWidth",
              "tiling__format",
            ],
            rows: [[1, "World", "Tiling", "tiles/root", 256, "Png"]],
          },
        ],
        connectedSessions,
        sendToSession,
      );
    },
  }),
}));

const { runWithSync } = await import("./query-sync");
const { MAX_LIVE_SYNC_DELTA_ROWS, MAX_LIVE_SYNC_FANOUT_RECIPIENTS, MAX_LIVE_SYNC_DELTA_PAYLOAD_BYTES } = await import("./query-sync");
const { loadSchemaFromDatabase } = await import("./schema");

beforeEach(() => {
  introspectionResult = { schema_source: "test schema" };
  sessionIds = ["s1"];
  reshapedRows = [[1, "World", { type: "Tiling", tileRootKey: "tiles/root", tileWidth: 256, format: { type: "Png" } }]];
});

const schemaDb = {
  execute: mock(async (sql: string) => {
    if (sql.includes("is_initialized")) {
      return { rows: [{ is_initialized: 1 }] };
    }

    return { rows: [{ result: JSON.stringify(introspectionResult) }] };
  }),
};

test("runWithSync sends reshaped sync deltas", async () => {
  await loadSchemaFromDatabase(schemaDb as any);

  const result = await runWithSync(
    {} as any,
    {} as any,
    "query-id",
    {},
    {},
    new Map([["s1", { session: {} }]]),
  );

  expect(result.kind).toBe("success");

  const sent: Array<{ sessionId: string; message: unknown }> = [];
  await result.sync((sessionId, message) => {
    sent.push({ sessionId, message });
  });

  expect(sent).toEqual([
    {
      sessionId: "s1",
      message: {
        type: "delta",
        data: [
          {
            table_name: "maps",
            headers: ["id", "name", "tiling"],
            rows: [[1, "World", { type: "Tiling", tileRootKey: "tiles/root", tileWidth: 256, format: { type: "Png" } }]],
          },
        ],
      },
    },
  ]);
});

test("runWithSync stamps sync deltas with databaseId", async () => {
  introspectionResult = { schema_source: "campaign schema" };
  await loadSchemaFromDatabase("campaign:123", schemaDb as any);

  const result = await runWithSync(
    {} as any,
    {} as any,
    "query-id",
    {},
    {},
    new Map([["s1", { session: {} }]]),
    "campaign:123",
  );

  const sent: Array<{ sessionId: string; message: any }> = [];
  await result.sync((sessionId, message) => {
    sent.push({ sessionId, message });
  });

  expect(sent[0].message.databaseId).toBe("campaign:123");
});

test("runWithSync sends syncRequired when delta row count exceeds cap", async () => {
  reshapedRows = Array.from({ length: MAX_LIVE_SYNC_DELTA_ROWS + 1 }, (_, index) => [index, "World", null]);
  await loadSchemaFromDatabase(schemaDb as any);

  const result = await runWithSync(
    {} as any,
    {} as any,
    "query-id",
    {},
    {},
    new Map([["s1", { session: {} }]]),
  );

  const sent: Array<{ sessionId: string; message: any }> = [];
  await result.sync((sessionId, message) => {
    sent.push({ sessionId, message });
  });

  expect(sent).toEqual([{ sessionId: "s1", message: { type: "syncRequired" } }]);
});

test("runWithSync sends syncRequired when fanout recipient count exceeds cap", async () => {
  sessionIds = Array.from({ length: MAX_LIVE_SYNC_FANOUT_RECIPIENTS + 1 }, (_, index) => `s${index}`);
  await loadSchemaFromDatabase(schemaDb as any);

  const result = await runWithSync(
    {} as any,
    {} as any,
    "query-id",
    {},
    {},
    new Map(sessionIds.map((sessionId) => [sessionId, { session: {} }])),
  );

  const sent: Array<{ sessionId: string; message: any }> = [];
  await result.sync((sessionId, message) => {
    sent.push({ sessionId, message });
  });

  expect(sent).toHaveLength(MAX_LIVE_SYNC_FANOUT_RECIPIENTS + 1);
  expect(sent.every((entry) => entry.message.type === "syncRequired")).toBe(true);
});

test("runWithSync sends syncRequired when payload bytes exceed cap", async () => {
  reshapedRows = [[1, "x".repeat(MAX_LIVE_SYNC_DELTA_PAYLOAD_BYTES), null]];
  await loadSchemaFromDatabase(schemaDb as any);

  const result = await runWithSync(
    {} as any,
    {} as any,
    "query-id",
    {},
    {},
    new Map([["s1", { session: {} }]]),
    "campaign:123",
  );

  const sent: Array<{ sessionId: string; message: any }> = [];
  await result.sync((sessionId, message) => {
    sent.push({ sessionId, message });
  });

  expect(sent).toEqual([{ sessionId: "s1", message: { type: "syncRequired", databaseId: "campaign:123" } }]);
});
