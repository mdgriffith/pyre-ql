import type { Client } from "@libsql/client";
import * as Ark from "arktype";
import { buildArgs, formatResultData, toSqlStatements, type SqlInfo } from "./sql";

type RunnerMeta = {
  session_args: string[];
  ReturnData: Ark.Type<any>;
};

function decodeOrThrow<T>(validator: Ark.Type<T>, data: unknown, label: string = "data"): T {
  const decoded = validator(data);
  if (decoded instanceof Ark.type.errors) {
    const errorStr = JSON.stringify(decoded, null, 2);
    throw new Error(`Failed to decode ${label}: ${errorStr}`);
  }
  return decoded as T;
}

export function toRunner<Input, Result>(meta: RunnerMeta, sql: SqlInfo[]) {
  return async (db: Client, session: Record<string, any>, input?: Input): Promise<Result> => {
    const args = buildArgs(
      input as Record<string, any> | undefined,
      session as Record<string, any>,
      meta.session_args,
    );
    const results = await db.batch(toSqlStatements(sql, args));
    const data = formatResultData(sql, results);
    return decodeOrThrow(meta.ReturnData, data, "return data");
  };
}
