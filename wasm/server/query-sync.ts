import { Client } from "@libsql/client";
import * as wasm from "../pkg/pyre_wasm.js";
import {
  run,
  type QueryMap,
  type QueryResult,
  type Session,
  type SessionValue,
  type SyncDeltasFn,
} from "./query";

const syncWithWasm: SyncDeltasFn = async (affectedRowGroups, connectedSessions, sendToSession) => {
  const deltasResult = wasm.calculate_sync_deltas(affectedRowGroups, connectedSessions);

  if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
    console.error("[SyncDeltas] Failed to calculate sync deltas:", deltasResult);
    return;
  }

  const result = typeof deltasResult === "string" ? JSON.parse(deltasResult) : deltasResult;

  for (const group of result.groups) {
    const deltaMessage = {
      type: "delta",
      data: group.table_groups,
    };

    for (const sessionId of group.session_ids) {
      sendToSession(sessionId, deltaMessage);
    }
  }
};

export async function runWithSync(
  db: Client,
  queryMap: QueryMap,
  queryId: string,
  args: any,
  executingSession: Session,
  connectedSessions?: Map<string, { session: Record<string, SessionValue>; [key: string]: any }>
): Promise<QueryResult> {
  return run(db, queryMap, queryId, args, executingSession, connectedSessions, syncWithWasm);
}
