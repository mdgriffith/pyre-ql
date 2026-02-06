import { Client, InStatement } from "@libsql/client";
import * as Ark from "arktype";
import { buildArgs, formatResultData, toSqlStatements, type SqlInfo } from "./runtime/sql";

export type SessionValue = null | number | string | Uint8Array;

/**
 * Query metadata containing all information needed to execute a query.
 */
export interface QueryMetadata {
    id: string;
    sql: SqlInfo[];
    session_args: string[];
    InputValidator: Ark.Type<any>;
    SessionValidator: Ark.Type<any>;
}

/**
 * Map of query IDs to their metadata.
 */
export interface QueryMap {
    [queryId: string]: QueryMetadata;
}

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
    /**
     * Broadcast sync deltas to connected clients.
     * Always present, but will be a no-op if there are no affected rows or no connected sessions.
     * 
     * @param sendToSession - Callback to send a message to a specific session
     * @example
     * ```typescript
     * await result.sync((sessionId, message) => {
     *   const client = connectedClients.get(sessionId);
     *   if (client?.ws.readyState === 1) {
     *     client.ws.send(JSON.stringify(message));
     *   }
     * });
     * ```
     */
    sync(sendToSession: (sessionId: string, message: any) => void): Promise<void>;
}

export type SyncDeltasFn = (
    affectedRowGroups: any[],
    connectedSessions: Map<string, { session: Record<string, SessionValue>; [key: string]: any }>,
    sendToSession: (sessionId: string, message: any) => void
) => Promise<void>;

function extractAffectedRowGroups(sql: SqlInfo[], resultSets: any[]): any[] {
    const groups: any[] = [];
    const includedResultSets = resultSets.filter((_, index) => sql[index]?.include);

    for (const resultSet of includedResultSets) {
        if (!resultSet?.columns?.length) {
            continue;
        }

        const colName = resultSet.columns[0];
        if (colName !== "_affectedRows") {
            continue;
        }

        for (const row of resultSet.rows || []) {
            if (!(colName in row)) {
                continue;
            }

            const raw = row[colName];
            let parsed: any;

            if (typeof raw === "string") {
                parsed = JSON.parse(raw);
            } else {
                parsed = raw;
            }

            if (Array.isArray(parsed)) {
                groups.push(...parsed);
            } else if (parsed != null) {
                groups.push(parsed);
            }
        }
    }

    return groups;
}


function decodeOrError<T>(validator: Ark.Type<T>, data: unknown, context: string): { valid: boolean; error?: string; value?: T } {
    const decoded = validator(data);
    if (decoded instanceof Ark.type.errors) {
        const errorStr = JSON.stringify(decoded, null, 2);
        return { valid: false, error: `${context}: ${errorStr}` };
    }
    return { valid: true, value: decoded as T };
}

/**
 * Execute a query using the provided query map and database client.
 * 
 * @param db - The database client (already connected)
 * @param queryMap - Map of query IDs to query metadata
 * @param queryId - The query ID to execute
 * @param args - Query arguments
 * @param executingSession - The session executing the query
 * @param connectedSessions - Map of all connected sessions (for sync delta calculation)
 * @returns Query result with response and sync function (always present)
 * @example
 * ```typescript
 * import { run } from "pyre-wasm/server";
 * import { queries } from "./generated/typescript/targets/server/queries";
 * const result = await run(db, queries, "createPost", args, session, connectedClients);
 * await result.sync((sessionId, message) => { ... });
 * ```
 */
export async function run(
    db: Client,
    queryMap: QueryMap,
    queryId: string,
    args: any,
    executingSession: Session,
    connectedSessions?: Map<string, { session: Record<string, SessionValue>;[key: string]: any }>,
    syncDeltas?: SyncDeltasFn
): Promise<QueryResult> {
    // Look up query metadata
    const query = queryMap[queryId];
    if (!query) {
        return {
            kind: "error",
            error: {
                errorType: "UnknownQuery",
                message: `Unknown query ID: ${queryId}`,
            },
            async sync() { },
        };
    }

    // Validate input
    const inputValidation = decodeOrError(query.InputValidator, args, "Input");
    if (!inputValidation.valid) {
        return {
            kind: "error",
            error: {
                errorType: "InvalidInput",
                message: inputValidation.error || "Invalid input",
            },
            async sync() { },
        };
    }

    // Validate session
    const sessionValidation = decodeOrError(query.SessionValidator, executingSession, "Session");
    if (!sessionValidation.valid) {
        return {
            kind: "error",
            error: {
                errorType: "InvalidSession",
                message: sessionValidation.error || "Invalid session",
            },
            async sync() { },
        };
    }

    // Prepare arguments
    const validatedInput = inputValidation.value ?? {};
    const validatedSession = sessionValidation.value ?? {};
    const validArgs = buildArgs(
        validatedInput as Record<string, any>,
        validatedSession as Record<string, any>,
        query.session_args
    );

    // Prepare SQL statements
    const sqlStatements: InStatement[] = toSqlStatements(query.sql, validArgs);

    // Execute query
    const resultSets = await db.batch(sqlStatements);
    const affectedRowGroups: any[] = extractAffectedRowGroups(query.sql, resultSets);
    const response = formatResultData(query.sql, resultSets);

    // Always create sync function - it will be a no-op if there's nothing to send
    /**
     * Broadcast sync deltas to connected clients.
     * 
     * For each session group, sends filtered table groups.
     * Clients receive only the rows they have permission to see.
     * 
     * Message format sent to each client (grouped by table for efficiency):
     * ```json
     * [
     *   {
     *     "table_name": "users",
     *     "headers": ["id", "name"],
     *     "rows": [[1, "Alice"], [2, "Bob"]]
     *   },
     *   {
     *     "table_name": "posts",
     *     "headers": ["id", "title"],
     *     "rows": [[10, "Hello"], [11, "World"]]
     *   }
     * ]
     * ```
     */
    async function sync(sendToSession: (sessionId: string, message: any) => void): Promise<void> {
        // Early return if nothing to sync
        if (affectedRowGroups.length === 0 || !connectedSessions || connectedSessions.size === 0) {
            return;
        }

        if (!syncDeltas) {
            return;
        }

        await syncDeltas(affectedRowGroups, connectedSessions, sendToSession);
    }

    return {
        kind: "success",
        response,
        sync,
    };
}
