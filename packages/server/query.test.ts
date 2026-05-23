import { expect, mock, test } from "bun:test";
import { z } from "zod";
import { run } from "./query";

test("sync wraps mutation responses with server revision metadata", async () => {
  const db = {
    batch: mock(async () => ([
      {
        columns: ["createdNote"],
        rows: [{ createdNote: JSON.stringify({ id: 1, body: "one" }) }],
      },
      {
        columns: ["_affectedRows"],
        rows: [{ _affectedRows: JSON.stringify([{ table_name: "notes", headers: ["id"], rows: [[1]] }]) }],
      },
    ])),
  };

  const result = await run(
    db as any,
    {
      createNote: {
        id: "createNote",
        sql: [
          { include: true, params: [], sql: "select createdNote" },
          { include: true, params: [], sql: "select _affectedRows" },
        ],
        session_args: [],
        optional_input_args: [],
        json_input_args: [],
        InputValidator: z.object({}),
        SessionValidator: z.object({}),
      },
    },
    "createNote",
    {},
    {},
    new Map(),
    async () => ({ serverRevision: 42 }),
  );

  await result.sync(() => {});

  expect(result.response).toEqual({
    serverRevision: 42,
    result: {
      createdNote: [{ id: 1, body: "one" }],
    },
  });
});

test("sync mode omits normal mutation result", async () => {
  const db = {
    batch: mock(async () => ([
      {
        columns: ["_affectedRows"],
        rows: [{ _affectedRows: JSON.stringify([{ table_name: "notes", headers: ["id"], rows: [[1]] }]) }],
      },
    ])),
  };

  const result = await run(
    db as any,
    {
      createNote: {
        id: "createNote",
        sql: [
          { include: true, params: [], sql: "select createdNote" },
        ],
        syncSql: [
          { include: true, params: [], sql: "select _affectedRows" },
        ],
        session_args: [],
        optional_input_args: [],
        json_input_args: [],
        InputValidator: z.object({}),
        SessionValidator: z.object({}),
      },
    },
    "createNote",
    {},
    {},
    new Map(),
    async () => ({ serverRevision: 42, originMessage: { type: "delta" } }),
    undefined,
    { mode: "sync" },
  );

  await result.sync(() => {});

  expect(result.response).toEqual({
    serverRevision: 42,
    sync: { type: "delta" },
  });
  expect(db.batch).toHaveBeenCalledWith([{ sql: "select _affectedRows", args: {} }]);
});
