import { Client } from "@libsql/client";
import * as wasm from "./wasm/pyre_wasm.js";
import { normalizeForWasmJson } from "./wasm-json";
import { requireDatabaseId, type DatabaseId } from "./database-id";
import { activateSchemaForDatabase } from "./schema";
import {
  run,
  type QueryMap,
  type QueryResult,
  type Session,
  type SessionValue,
  type SyncDeltasFn,
} from "./query";

function syncWithWasmForDatabase(databaseId?: DatabaseId): SyncDeltasFn {
  const normalizedDatabaseId = databaseId ? requireDatabaseId(databaseId) : undefined;

  return async (affectedRowGroups, connectedSessions, sendToSession) => {
    activateSchemaForDatabase(normalizedDatabaseId);

    const deltasResult = wasm.calculate_sync_deltas(affectedRowGroups, connectedSessions);

    if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
      console.error("[SyncDeltas] Failed to calculate sync deltas:", deltasResult);
      return;
    }

    const result = typeof deltasResult === "string" ? JSON.parse(deltasResult) : deltasResult;

    for (const group of result.groups) {
      const reshapedTableGroupsResult = wasm.reshape_sync_table_groups(normalizeForWasmJson(group.table_groups));

      if (typeof reshapedTableGroupsResult === "string" && reshapedTableGroupsResult.startsWith("Error:")) {
        console.error("[SyncDeltas] Failed to reshape sync deltas:", reshapedTableGroupsResult);
        continue;
      }

      const deltaMessage = {
        type: "delta",
        ...(normalizedDatabaseId ? { databaseId: normalizedDatabaseId } : {}),
        data: typeof reshapedTableGroupsResult === "string"
          ? JSON.parse(reshapedTableGroupsResult)
          : reshapedTableGroupsResult,
      };

      for (const sessionId of group.session_ids) {
        sendToSession(sessionId, deltaMessage);
      }
    }
  };
}

const syncWithWasm: SyncDeltasFn = syncWithWasmForDatabase();

export async function runWithSync(
  db: Client,
  queryMap: QueryMap,
  queryId: string,
  args: any,
  executingSession: Session,
  connectedSessions?: Map<string, { session: Record<string, SessionValue>; [key: string]: any }>,
  databaseId?: DatabaseId,
): Promise<QueryResult> {
  return run(db, queryMap, queryId, args, executingSession, connectedSessions, databaseId ? syncWithWasmForDatabase(databaseId) : syncWithWasm);
}
