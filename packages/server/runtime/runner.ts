import type { Client } from "@libsql/client";
import type { ZodType } from "zod";
import { buildArgs, formatResultData, toSqlStatements, type SqlInfo } from "./sql";

type Validator<T> = ZodType<T>;

type RunnerMeta = {
  session_args: string[];
  ReturnData: Validator<any>;
};

function decodeOrThrow<T>(validator: Validator<T>, data: unknown, label: string = "data"): T {
  const parsed = validator.safeParse(data);
  if (!parsed.success) {
    throw new Error(`Failed to decode ${label}: ${String(parsed.error)}`);
  }
  return parsed.data;
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
