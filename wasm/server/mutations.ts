/**
 * Represents a row that was affected by a mutation.
 */
export interface AffectedRow {
    table_name: string;
    row: Record<string, any>;
    headers: string[];
}

/**
 * Extract affected rows from a mutation result.
 * Mutations return affected rows in the `_affectedRows` field of the result data.
 * 
 * @param resultData - The result data from a mutation query
 * @returns Array of affected rows, empty if no affected rows found
 * @example
 * ```typescript
 * const result = await runQuery(db, path, url, "createPost", args, session, connectedSessions);
 * const affectedRows = extractAffectedRows(result.response);
 * ```
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
