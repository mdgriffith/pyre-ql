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
    console.log("RESULT  ", result);
    console.log(JSON.stringify(result.data));
    // return c.json(result.data.map((d) => d.rows));
    // return c.json(result.data.map((d) => JSON.parse(d.rows)));

    return c.json(result.data.map((d) =>
      // This is an awkward conversion because the sql is returning stringified json
      // key: {stringified-json}  
      d.rows.map((r) => {
        let cleaned_row: any = {};
        for (const column in d.columns) {
          if (column in r) {
            let col = r[column];
            if (typeof col == "string") {
              cleaned_row[column] = JSON.parse(col)
            }
          }
        }
        return cleaned_row
      })
    ));
  }
  console.log(result);
  c.status(500);
  return c.json({ error: result.message });
});
export default app;
