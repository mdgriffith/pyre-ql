import { Hono } from "hono";
import * as Query from "../pyre/generated/typescript/targets/server/queries";

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
    url: `file:${process.cwd()}/db/playground.db`,
    authToken: undefined,
  };

  const session = {}

  const result = await Query.run(env, req, session, args);

  if (result.kind === "success") {
    console.log("RESULT  ", result);
    return c.json(result.data.map((d) => d.rows));
  }
  console.log(result);
  c.status(500);
  return c.json({ error: result.message });
});
export default app;
