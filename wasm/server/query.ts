import { Client } from "@libsql/client";
import { resolve } from "path";
// Import WASM module from the same package
import * as wasm from "../pkg/pyre_wasm.js";
import { extractAffectedRows } from "./mutations";

export type SessionValue = null | number | string | Uint8Array;

/**
 * Session data structure - can be any object with string keys.
 */
export interface Session {
    [key: string]: any;
}

/**
 * Connected session for sync delta calculation.
 */
export interface ConnectedSession {
    session_id: string;
    fields: Record<string, SessionValue>;
}

/**
 * Result of executing a query.
 */
export interface QueryResult {
    kind: "success" | "error";
    /** The JSON response to return to the client (only present on success) */
    response?: any;
    /** Error details (only present on error) */
    error?: {
        errorType: string;
        message: string;
    };
    /** Sync deltas handler - always present, no-op if there's nothing to sync */
    syncDeltas: SyncDeltas;
}

/**
 * Handler for broadcasting sync deltas to connected clients.
 * Always present on QueryResult, but will be a no-op if there are no affected rows
 * or no connected sessions.
 */
export interface SyncDeltas {
    /**
     * Broadcast sync deltas to connected clients.
     * If there are no affected rows or no connected sessions, this is a no-op.
     * 
     * @param sendToSession - Callback to send a message to a specific session
     * @example
     * ```typescript
     * await result.syncDeltas.sync((sessionId, message) => {
     *   const client = connectedClients.get(sessionId);
     *   if (client?.ws.readyState === 1) {
     *     client.ws.send(JSON.stringify(message));
     *   }
     * });
     * ```
     */
    sync(sendToSession: (sessionId: string, message: any) => void): Promise<void>;
}

// Dynamic import cache for query module
let QueryModule: any = null;

async function getQueryModule(queryModulePath: string) {
    if (!QueryModule) {
        QueryModule = await import(queryModulePath);
    }
    return QueryModule;
}

/**
 * Execute a query and optionally calculate sync deltas for mutations.
 * 
 * @param db - The database client
 * @param queryModulePath - Path to the generated query module (e.g., "../query")
 * @param dbUrl - The database URL string (e.g., "file:./test.db")
 * @param queryId - The query ID to execute
 * @param args - Query arguments
 * @param executingSession - The session executing the query
 * @param connectedSessions - Map of all connected sessions (for sync delta calculation)
 * @returns Query result with response and syncDeltas (always present)
 * @example
 * ```typescript
 * import { runQuery } from "pyre-wasm/server";
 * const result = await runQuery(db, "../query", "file:./test.db", "createPost", args, session, connectedClients);
 * ```
 */
export async function runQuery(
    db: Client,
    queryModulePath: string,
    dbUrl: string,
    queryId: string,
    args: any,
    executingSession: Session,
    connectedSessions?: Map<string, { fields: Record<string, SessionValue> }>
): Promise<QueryResult> {
    const Query = await getQueryModule(queryModulePath);
    const env = {
        url: dbUrl,
        authToken: undefined,
    };

    // Execute the query
    const result = await Query.run(env, queryId, executingSession, args);

    if (result.kind === "error") {
        // Even on error, provide syncDeltas (no-op)
        return {
            kind: "error",
            error: {
                errorType: result.errorType,
                message: result.message || "Query execution failed",
            },
            syncDeltas: {
                async sync() {
                    // No-op: nothing to sync on error
                },
            },
        };
    }

    // Success case
    const response = result.data;

    // Extract affected rows (may be empty for queries)
    const affectedRows = extractAffectedRows(response);

    // Convert connected sessions to format expected by calculate_sync_deltas
    const connectedSessionsArray: ConnectedSession[] = connectedSessions
        ? Array.from(connectedSessions.entries()).map(
            ([sessionId, client]) => ({
                session_id: sessionId,
                fields: client.fields,
            })
        )
        : [];

    // Always create syncDeltas - it will be a no-op if there's nothing to send
    const syncDeltas: SyncDeltas = {
        async sync(sendToSession: (sessionId: string, message: any) => void): Promise<void> {
            // Early return if nothing to sync
            if (affectedRows.length === 0 || connectedSessionsArray.length === 0) {
                return;
            }

            // Calculate sync deltas
            const deltasResult = wasm.calculate_sync_deltas(affectedRows, connectedSessionsArray);

            if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
                console.error("[SyncDeltas] Failed to calculate sync deltas:", deltasResult);
                return;
            }

            const result = typeof deltasResult === "string" ? JSON.parse(deltasResult) : deltasResult;

            // Broadcast to each group
            for (const group of result.groups) {
                const deltaMessage = {
                    type: "delta",
                    data: {
                        all_affected_rows: result.all_affected_rows,
                        affected_row_indices: group.affected_row_indices,
                    },
                };

                for (const sessionId of group.session_ids) {
                    sendToSession(sessionId, deltaMessage);
                }
            }
        },
    };

    return {
        kind: "success",
        response,
        syncDeltas,
    };
}
