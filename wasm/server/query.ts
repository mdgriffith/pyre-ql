import { Client, InStatement } from "@libsql/client";
import * as wasm from "../pkg/pyre_wasm.js";

export type SessionValue = null | number | string | Uint8Array;

/**
 * Represents a row that was affected by a mutation.
 */
export interface AffectedRow {
    table_name: string;
    row: Record<string, any>;
    headers: string[];
}

/**
 * SQL statement information for a query.
 */
export interface SqlInfo {
    include: boolean;
    params: string[];
    sql: string;
}

/**
 * Schema definition for validation.
 * Maps field names to their type strings (e.g., "string", "number", "boolean", "string?", "number[]")
 */
export interface Schema {
    [fieldName: string]: string;
}

/**
 * Query metadata containing all information needed to execute a query.
 */
export interface QueryMetadata {
    id: string;
    sql: SqlInfo[];
    session_args: string[];
    input_schema: Schema;
    session_schema: Schema;
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

/**
 * Extract affected rows from a mutation result.
 * Mutations return affected rows in the `_affectedRows` field of the result data.
 */
export function extractAffectedRows(resultData: any): AffectedRow[] {
    const affectedRows: AffectedRow[] = [];

    if (!resultData || !resultData._affectedRows) {
        return affectedRows;
    }

    const affectedRowsValue = resultData._affectedRows;

    let affectedRowsArray: any[];
    if (typeof affectedRowsValue === "string") {
        affectedRowsArray = JSON.parse(affectedRowsValue);
    } else if (Array.isArray(affectedRowsValue)) {
        affectedRowsArray = affectedRowsValue;
    } else {
        return affectedRows;
    }

    for (const tableGroup of affectedRowsArray) {
        let groupData: any;
        if (typeof tableGroup === "string") {
            groupData = JSON.parse(tableGroup);
        } else {
            groupData = tableGroup;
        }

        // New format: { table_name, headers, rows: [[...], [...]] }
        if (groupData.table_name && groupData.headers && groupData.rows) {
            const tableName = groupData.table_name;
            const headers = groupData.headers;
            const rows = groupData.rows;

            for (const rowArray of rows) {
                const rowObject: Record<string, any> = {};
                for (let i = 0; i < headers.length && i < rowArray.length; i++) {
                    rowObject[headers[i]] = rowArray[i];
                }

                affectedRows.push({
                    table_name: tableName,
                    row: rowObject,
                    headers: headers,
                });
            }
        }
        // Legacy format: { table_name, row: {...}, headers } (for backwards compatibility)
        else if (groupData.table_name && groupData.row && groupData.headers) {
            affectedRows.push({
                table_name: groupData.table_name,
                row: groupData.row,
                headers: groupData.headers,
            });
        }
    }

    return affectedRows;
}

/**
 * Parse a type string (e.g., "string", "number?", "boolean[]") into type info.
 */
function parseType(typeStr: string): { baseType: string; nullable: boolean; isArray: boolean } {
    const nullable = typeStr.endsWith("?");
    const isArray = typeStr.endsWith("[]");
    let baseType = typeStr;

    if (nullable) {
        baseType = baseType.slice(0, -1);
    }
    if (isArray) {
        baseType = baseType.slice(0, -2);
    }

    return { baseType, nullable, isArray };
}

/**
 * Validate a value against a type string.
 */
function validateValue(value: any, typeStr: string): { valid: boolean; error?: string } {
    const { baseType, nullable, isArray } = parseType(typeStr);

    if (value === null || value === undefined) {
        if (nullable) {
            return { valid: true };
        }
        return { valid: false, error: `Expected ${typeStr}, got null or undefined` };
    }

    if (isArray) {
        if (!Array.isArray(value)) {
            return { valid: false, error: `Expected array, got ${typeof value}` };
        }
        // Validate array elements
        for (let i = 0; i < value.length; i++) {
            const elemResult = validateValue(value[i], baseType);
            if (!elemResult.valid) {
                return { valid: false, error: `Array element at index ${i}: ${elemResult.error}` };
            }
        }
        return { valid: true };
    }

    switch (baseType) {
        case "string":
            if (typeof value !== "string") {
                return { valid: false, error: `Expected string, got ${typeof value}` };
            }
            break;
        case "number":
            if (typeof value !== "number" || isNaN(value)) {
                return { valid: false, error: `Expected number, got ${typeof value}` };
            }
            break;
        case "boolean":
            if (typeof value !== "boolean") {
                return { valid: false, error: `Expected boolean, got ${typeof value}` };
            }
            break;
        default:
            // For unknown types, just check it's not null/undefined
            break;
    }

    return { valid: true };
}

/**
 * Validate an object against a schema.
 */
function validateSchema(obj: any, schema: Schema, context: string): { valid: boolean; error?: string } {
    if (obj === null || obj === undefined) {
        return { valid: false, error: `${context} is null or undefined` };
    }

    if (typeof obj !== "object" || Array.isArray(obj)) {
        return { valid: false, error: `${context} must be an object` };
    }

    for (const [fieldName, typeStr] of Object.entries(schema)) {
        const { nullable } = parseType(typeStr);
        const value = obj[fieldName];

        if (value === undefined) {
            if (!nullable) {
                return { valid: false, error: `${context} missing required field: ${fieldName}` };
            }
            continue;
        }

        const result = validateValue(value, typeStr);
        if (!result.valid) {
            return { valid: false, error: `${context}.${fieldName}: ${result.error}` };
        }
    }

    return { valid: true };
}

/**
 * Convert session object to SQL parameter format.
 */
function toSessionArgs(sessionArgs: string[], session: Session): Record<string, any> {
    const result: Record<string, any> = {};

    if (session == null) {
        return result;
    }

    for (const key of sessionArgs) {
        if (key in session) {
            result[`session_${key}`] = session[key];
        }
    }

    return result;
}

/**
 * Stringify nested objects (but not arrays or primitives).
 */
function stringifyNestedObjects(obj: Record<string, any>): Record<string, any> {
    const result: Record<string, any> = {};

    for (const key in obj) {
        if (obj.hasOwnProperty(key)) {
            const value = obj[key];
            if (typeof value === "object" && value !== null && !Array.isArray(value)) {
                result[key] = JSON.stringify(value);
            } else {
                result[key] = value;
            }
        }
    }

    return result;
}

/**
 * Filter result sets to only include those marked with include: true.
 */
function onlyIncluded(sqlItems: SqlInfo[], resultSets: any[]): any[] {
    return resultSets.filter((_, index) => sqlItems[index]?.include);
}

/**
 * Format query result data from database response.
 */
function formatResultData(sqlItems: SqlInfo[], resultSets: any[]): any {
    const formatted: any = {};

    for (const resultSet of onlyIncluded(sqlItems, resultSets)) {
        if (resultSet.columns.length < 1) {
            continue;
        }

        const colName = resultSet.columns[0];

        for (const row of resultSet.rows) {
            if (colName in row && typeof row[colName] === "string") {
                const parsed = JSON.parse(row[colName]);
                if (Array.isArray(parsed)) {
                    formatted[colName] = parsed;
                } else {
                    formatted[colName] = [parsed];
                }
                break; // Only process first row
            }
        }
    }

    return formatted;
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
 * @returns Query result with response and syncDeltas (always present)
 * @example
 * ```typescript
 * import { runQuery } from "pyre-wasm/server";
 * import { queries } from "./generated/server/typescript/query";
 * const result = await runQuery(db, queries, "createPost", args, session, connectedClients);
 * ```
 */
export async function runQuery(
    db: Client,
    queryMap: QueryMap,
    queryId: string,
    args: any,
    executingSession: Session,
    connectedSessions?: Map<string, { fields: Record<string, SessionValue> }>
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
            syncDeltas: {
                async sync() { },
            },
        };
    }

    // Validate input
    const inputValidation = validateSchema(args, query.input_schema, "Input");
    if (!inputValidation.valid) {
        return {
            kind: "error",
            error: {
                errorType: "InvalidInput",
                message: inputValidation.error || "Invalid input",
            },
            syncDeltas: {
                async sync() { },
            },
        };
    }

    // Validate session
    const sessionValidation = validateSchema(executingSession, query.session_schema, "Session");
    if (!sessionValidation.valid) {
        return {
            kind: "error",
            error: {
                errorType: "InvalidSession",
                message: sessionValidation.error || "Invalid session",
            },
            syncDeltas: {
                async sync() { },
            },
        };
    }

    // Prepare arguments
    const sessionArgs = toSessionArgs(query.session_args, executingSession);
    const validArgs = stringifyNestedObjects({ ...args, ...sessionArgs });

    // Prepare SQL statements
    const sqlStatements: InStatement[] = query.sql.map(({ params, sql }) => {
        const filteredArgs: Record<string, any> = {};
        for (const key of params) {
            if (key in validArgs) {
                filteredArgs[key] = validArgs[key];
            }
        }
        return { sql, args: filteredArgs };
    });

    // Execute query
    const resultSets = await db.batch(sqlStatements);
    const response = formatResultData(query.sql, resultSets);

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
