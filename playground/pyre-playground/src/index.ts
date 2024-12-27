import { Hono } from "hono";
import * as Query from "../pyre/generated/typescript/query";

const app = new Hono();

app.get("/", (c) => {
  return c.text("Hello Hono!");
});

app.post("/db/:req", async (c) => {
  // Body args
  const { req } = c.req.param();
  const args = await c.req.json();

  console.log("RECEIVED ARGS", req, args);

  const env = {
    url: "file:/Users/mattgriffith/projects/mdgriffith/pyre-ql/playground/pyre-playground/test.db",
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
