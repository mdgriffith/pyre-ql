// @ts-nocheck
import { expect, mock, test } from "bun:test";

mock.module("./wasm/pyre_wasm.js", () => ({
  get_sync_status_sql: () => "select 1",
  get_sync_sql: () => ({ tables: [] }),
  calculate_sync_deltas: () => ({
    groups: [
      {
        session_ids: ["s1"],
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
      rows: [[1, "World", { type: "Tiling", tileRootKey: "tiles/root", tileWidth: 256, format: { type: "Png" } }]],
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

test("runWithSync sends reshaped sync deltas", async () => {
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
