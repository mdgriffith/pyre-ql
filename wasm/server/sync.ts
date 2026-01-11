import { Client } from "@libsql/client";
// Import WASM module from the same package
import * as wasm from "../pkg/pyre_wasm.js";

export type SessionValue = null | number | string | Uint8Array;

/**
 * Sync cursor tracks the last seen state for each table.
 */
export interface SyncCursor {
    tables: Record<string, {
        last_seen_updated_at: number | null;
        permission_hash: string;
    }>;
}

/**
 * Result of a sync page request.
 */
export interface SyncPageResult {
    tables: Record<
        string,
        {
            rows: any[];
            permission_hash: string;
            last_seen_updated_at: number | null;
        }
    >;
    has_more: boolean;
}

/**
 * Session data for sync operations.
 */
export interface SyncSession {
    fields: Record<string, SessionValue>;
}

/**
 * Handle a sync request, returning data that needs to be synced.
 * 
 * @param db - The database client
 * @param syncCursor - Current sync cursor state
 * @param session - Session data for permission evaluation
 * @param pageSize - Number of rows to fetch per table (default: 1000)
 * @returns Sync page result with data to sync
 * @example
 * ```typescript
 * import { handleSync } from "pyre-wasm/server";
 * const result = await handleSync(db, syncCursor, session, 1000);
 * ```
 */
export async function handleSync(
    db: Client,
    syncCursor: SyncCursor,
    session: SyncSession,
    pageSize: number = 1000
): Promise<SyncPageResult> {
    // Step 1: Get sync status SQL
    const statusSql = wasm.get_sync_status_sql(syncCursor, session);
    if (typeof statusSql === "string" && statusSql.startsWith("Error:")) {
        throw new Error(statusSql);
    }

    // Step 2: Execute sync status SQL
    const statusResult = await db.execute(statusSql as string);

    // Step 3: Get sync SQL for tables that need syncing
    const syncSqlResult = wasm.get_sync_sql(statusResult.rows, syncCursor, session, pageSize);
    if (typeof syncSqlResult === "string" && syncSqlResult.startsWith("Error:")) {
        throw new Error(syncSqlResult);
    }

    const sqlResult =
        typeof syncSqlResult === "string"
            ? JSON.parse(syncSqlResult)
            : syncSqlResult;

    const result: SyncPageResult = {
        tables: {},
        has_more: false,
    };

    // Collect all SQL statements for batch execution
    const allSqlStatements: string[] = [];
    for (const tableSql of sqlResult.tables) {
        allSqlStatements.push(...tableSql.sql);
    }

    // Execute all SQL statements in a single batch
    const allQueryResults = await db.batch(allSqlStatements);

    // Process results for each table
    let resultIndex = 0;
    for (const tableSql of sqlResult.tables) {
        const updatedAtIndex = tableSql.headers.indexOf("updatedAt");
        const tableRows: any[] = [];
        let maxUpdatedAt: number | null = null;

        for (const sql of tableSql.sql) {
            const queryResult = allQueryResults[resultIndex++];
            const columns = queryResult.columns;
            const rows = queryResult.rows || [];

            for (const row of rows) {
                const rowObject: Record<string, any> = {};
                for (const column of columns) {
                    rowObject[column] = row[column];
                }
                tableRows.push(rowObject);

                if (updatedAtIndex >= 0 && rowObject.updatedAt !== null && rowObject.updatedAt !== undefined) {
                    const updatedAt = typeof rowObject.updatedAt === "number"
                        ? rowObject.updatedAt
                        : new Date(rowObject.updatedAt).getTime() / 1000;
                    if (maxUpdatedAt === null || updatedAt > maxUpdatedAt) {
                        maxUpdatedAt = updatedAt;
                    }
                }
            }
        }

        const hasMoreForTable = tableRows.length > pageSize;
        const finalRows = hasMoreForTable ? tableRows.slice(0, pageSize) : tableRows;

        if (hasMoreForTable && updatedAtIndex >= 0 && finalRows.length > 0) {
            const lastRow = finalRows[finalRows.length - 1];
            const lastUpdatedAt = lastRow.updatedAt;
            if (lastUpdatedAt !== null && lastUpdatedAt !== undefined) {
                const updatedAt = typeof lastUpdatedAt === "number"
                    ? lastUpdatedAt
                    : new Date(lastUpdatedAt).getTime() / 1000;
                maxUpdatedAt = updatedAt;
            }
        }

        result.tables[tableSql.table_name] = {
            rows: finalRows,
            permission_hash: tableSql.permission_hash,
            last_seen_updated_at: maxUpdatedAt,
        };

        if (hasMoreForTable) {
            result.has_more = true;
        }
    }

    return result;
}
