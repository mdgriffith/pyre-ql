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

export const MAX_LIVE_SYNC_DELTA_ROWS = 5000;
export const MAX_LIVE_SYNC_DELTA_PAYLOAD_BYTES = 1024 * 1024;
export const MAX_LIVE_SYNC_FANOUT_RECIPIENTS = 1000;

async function nextLiveSyncRevision(db: Client): Promise<number> {
  const result = await db.execute("update _pyre_sync set value = value + 1 where key = 'server_revision' returning value");
  const value = result.rows[0]?.value;

  if (typeof value !== "number" && typeof value !== "bigint") {
    throw new Error("Failed to allocate Pyre sync server revision");
  }

  return Number(value);
}

function countRows(tableGroups: unknown): number {
  if (!Array.isArray(tableGroups)) {
    return 0;
  }

  return tableGroups.reduce((total, tableGroup) => {
    if (typeof tableGroup !== "object" || tableGroup == null || !("rows" in tableGroup)) {
      return total;
    }

    return total + (Array.isArray(tableGroup.rows) ? tableGroup.rows.length : 0);
  }, 0);
}

function liveSyncRequiresCatchup(message: unknown, rowCount: number, recipientCount: number): boolean {
  if (rowCount > MAX_LIVE_SYNC_DELTA_ROWS) {
    return true;
  }

  if (recipientCount > MAX_LIVE_SYNC_FANOUT_RECIPIENTS) {
    return true;
  }

  return new TextEncoder().encode(JSON.stringify(message)).byteLength > MAX_LIVE_SYNC_DELTA_PAYLOAD_BYTES;
}

function syncWithWasmForDatabase(db: Client, databaseId?: DatabaseId): SyncDeltasFn {
  const normalizedDatabaseId = databaseId ? requireDatabaseId(databaseId) : undefined;

  return async (affectedRowGroups, connectedSessions, sendToSession, originSessionId) => {
    activateSchemaForDatabase(normalizedDatabaseId);

    const deltasResult = wasm.calculate_sync_deltas(affectedRowGroups, connectedSessions);

    if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
      console.error("[SyncDeltas] Failed to calculate sync deltas:", deltasResult);
      return;
    }

    const serverRevision = await nextLiveSyncRevision(db);
    const result = typeof deltasResult === "string" ? JSON.parse(deltasResult) : deltasResult;

    if (!Array.isArray(result.groups) || result.groups.length === 0) {
      return { serverRevision };
    }

    let originMessage: unknown;

    for (const group of result.groups) {
      const reshapedTableGroupsResult = wasm.reshape_sync_table_groups(normalizeForWasmJson(group.table_groups));

      if (typeof reshapedTableGroupsResult === "string" && reshapedTableGroupsResult.startsWith("Error:")) {
        console.error("[SyncDeltas] Failed to reshape sync deltas:", reshapedTableGroupsResult);
        continue;
      }

      const data = typeof reshapedTableGroupsResult === "string"
        ? JSON.parse(reshapedTableGroupsResult)
        : reshapedTableGroupsResult;

      const deltaMessage = {
        type: "delta",
        serverRevision,
        ...(normalizedDatabaseId ? { databaseId: normalizedDatabaseId } : {}),
        data,
      };

      const message = liveSyncRequiresCatchup(deltaMessage, countRows(data), group.session_ids.length)
        ? {
          type: "syncRequired",
          serverRevision,
          ...(normalizedDatabaseId ? { databaseId: normalizedDatabaseId } : {}),
        }
        : deltaMessage;

      for (const sessionId of group.session_ids) {
        if (originSessionId && sessionId === originSessionId) {
          originMessage = message;
          continue;
        }
        sendToSession(sessionId, message);
      }
    }

    return { serverRevision, ...(originMessage === undefined ? {} : { originMessage }) };
  };
}

export async function runWithSync(
  db: Client,
  queryMap: QueryMap,
  queryId: string,
  args: any,
  executingSession: Session,
  connectedSessions?: Map<string, { session: Record<string, SessionValue>; [key: string]: any }>,
  databaseId?: DatabaseId,
  originSessionId?: string,
): Promise<QueryResult> {
  return run(db, queryMap, queryId, args, executingSession, connectedSessions, syncWithWasmForDatabase(db, databaseId), originSessionId);
}
