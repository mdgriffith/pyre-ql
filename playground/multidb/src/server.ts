import { Hono } from "hono";
import * as Query from "../pyre/generated/server/typescript/query";

const app = new Hono();

app.get("/", (c) => {
  return c.text(`
You're running a server with 🔥 Pyre 🔥 

Database Path: file:${process.cwd()}/db/playground.db
`);
});

app.post("/db/:req", async (c) => {
  // Body args
  const { req } = c.req.param();
  const args = await c.req.json();

  console.log("RECEIVED ARGS", req, args);

  const env = {
    Base: {
      id: `file:${process.cwd()}/db/base.db`,
      url: `file:${process.cwd()}/db/base.db`,
    },
    User: {
      id: `file:${process.cwd()}/db/user.db`,
      url: `file:${process.cwd()}/db/user.db`,
    },
  };

  const session = { userId: 6 }

  const result = await Query.run(env, req, session, args);

  if (result.kind === "success") {
    // console.log(JSON.stringify(result.data));
    // return c.json(result.data.map((d) => d.rows));
    // return c.json(result.data.map((d) => JSON.parse(d.rows)));

    const formatted: any = {}

    for (const result_set of result.data) {
      if (result_set.columns.length < 1) { continue }
      const col_name = result_set.columns[0];
      const gathered_rows = [];

      for (const row of result_set.rows) {
        if (col_name in row && typeof row[col_name] == 'string') {
          gathered_rows.push(JSON.parse(row[col_name]));
        }
      }

      formatted[col_name] = gathered_rows;

    }
    return c.json(formatted)
  } else {
    console.log(result);
    c.status(500);
    return c.json({ error: result.message });
  }
});



export default app;

