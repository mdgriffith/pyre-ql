import { Hono } from "hono";
import { createClient } from "@libsql/client";
import { join } from "path";
import { readFileSync } from "fs";
import init, {
    calculate_sync_deltas,
    sql_is_initialized,
    sql_introspect,
    sql_introspect_uninitialized,
    set_schema,
    get_sync_status_sql,
    get_sync_sql,
} from "../../../wasm/pkg/pyre_wasm.js";

// Initialize WASM
const wasmPath = join(process.cwd(), "..", "..", "wasm", "pkg", "pyre_wasm_bg.wasm");
const wasmBuffer = readFileSync(wasmPath);
await init({ wasm: wasmBuffer });

const app = new Hono();

// Types
interface ConnectedClient {
    sessionId: string;
    session: {
        fields: Record<string, SessionValue>;
    };
    ws: any; // Bun WebSocket
}

type SessionValue = null | number | string | Uint8Array;

interface AffectedRow {
    table_name: string;
    row: Record<string, any>;
    headers: string[];
}

interface ConnectedSession {
    session_id: string;
    fields: Record<string, SessionValue>;
}

const DB_PATH = join(process.cwd(), "test.db");
const connectedClients = new Map<string, ConnectedClient>();
let nextSessionId = 1;

// Load schema into WASM cache
async function loadSchema() {
    const db = createClient({
        url: `file:${DB_PATH}`,
    });

    const isInitializedQuery = sql_is_initialized();
    const isInitializedResult = await db.execute(isInitializedQuery);

    if (isInitializedResult.rows.length === 0) {
        throw new Error("Failed to check if database is initialized");
    }

    const isInitialized = isInitializedResult.rows[0].is_initialized === 1;

    let introspection;
    if (isInitialized) {
        const introspectionQuery = sql_introspect();
        const introspectionResult = await db.execute(introspectionQuery);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        introspection = JSON.parse(introspectionResult.rows[0].result as string);
    } else {
        const uninitializedIntrospect = sql_introspect_uninitialized();
        const introspectionResult = await db.execute(uninitializedIntrospect);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        introspection = JSON.parse(introspectionResult.rows[0].result as string);
    }

    set_schema(introspection);
    console.log("Schema loaded into WASM cache");
}

// Cache the Query module import
let QueryModule: any = null;

async function getQueryModule() {
    if (!QueryModule) {
        console.log("[Query] Importing query module for the first time...");
        try {
            // @ts-ignore - Generated file, may not exist until pyre generate is run
            QueryModule = await import("../pyre/generated/server/typescript/query");
            console.log("[Query] Query module imported successfully");
            console.log("[Query] Available exports:", Object.keys(QueryModule));
        } catch (error: any) {
            console.error("[Query] Failed to import query module:", error);
            console.error("[Query] Error details:", error.message);
            throw error;
        }
    }
    return QueryModule;
}

// Extract affected rows from mutation result
function extractAffectedRows(resultData: any): AffectedRow[] {
    const affectedRows: AffectedRow[] = [];

    // resultData is an object where keys are column names and values are arrays
    // Check if _affectedRows column exists
    if (!resultData || !resultData._affectedRows) {
        return affectedRows;
    }

    const affectedRowsValue = resultData._affectedRows;

    // Handle both string (needs parsing) and array cases
    let affectedRowsArray: any[];
    if (typeof affectedRowsValue === "string") {
        affectedRowsArray = JSON.parse(affectedRowsValue);
    } else if (Array.isArray(affectedRowsValue)) {
        affectedRowsArray = affectedRowsValue;
    } else {
        return affectedRows;
    }

    // Process each table group
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

            // Convert each row array to an object
            for (const rowArray of rows) {
                // Zip headers with row values to create an object
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

// Broadcast sync deltas to connected clients
async function broadcastSyncDeltas(affectedRows: AffectedRow[]) {
    console.log(`[Broadcast] Starting broadcast. Affected rows: ${affectedRows.length}, Connected clients: ${connectedClients.size}`);

    if (affectedRows.length === 0 || connectedClients.size === 0) {
        console.log(`[Broadcast] Skipping broadcast - no affected rows or no connected clients`);
        return;
    }

    // Convert connected clients to format expected by calculate_sync_deltas
    const connectedSessions: ConnectedSession[] = Array.from(connectedClients.values()).map(
        (client) => ({
            session_id: client.sessionId,
            fields: client.session.fields,
        })
    );

    console.log(`[Broadcast] Connected sessions:`, JSON.stringify(connectedSessions, null, 2));
    console.log(`[Broadcast] Affected rows:`, JSON.stringify(affectedRows, null, 2));

    // Debug: Log permission evaluation details
    for (const session of connectedSessions) {
        console.log(`[Broadcast] Evaluating permissions for session ${session.session_id}:`, JSON.stringify(session.fields, null, 2));
        for (const row of affectedRows) {
            console.log(`[Broadcast]   Row:`, JSON.stringify(row.row, null, 2));
            // Check if this row should be visible
            if (row.table_name === "posts") {
                const rowData = row.row as any;
                const sessionUserId = session.fields.userId;
                const authorUserId = rowData.authorUserId;
                const published = rowData.published;
                console.log(`[Broadcast]   Permission check: authorUserId (${authorUserId}) == Session.userId (${sessionUserId}) || published (${published}) == True (1)`);
                const authorMatch = authorUserId === sessionUserId;
                const publishedMatch = published === 1;
                const shouldSee = authorMatch || publishedMatch;
                console.log(`[Broadcast]   Result: authorMatch=${authorMatch}, publishedMatch=${publishedMatch}, shouldSee=${shouldSee}`);
            }
        }
    }

    // Calculate sync deltas
    const deltasResult = calculate_sync_deltas(affectedRows, connectedSessions);

    if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
        console.error("[Broadcast] Failed to calculate sync deltas:", deltasResult);
        return;
    }

    const result = typeof deltasResult === "string" ? JSON.parse(deltasResult) : deltasResult;
    console.log(`[Broadcast] Sync deltas result:`, JSON.stringify(result, null, 2));
    console.log(`[Broadcast] Number of groups:`, result.groups?.length || 0);

    // Broadcast to each group
    for (const group of result.groups) {
        const deltaMessage = {
            type: "delta",
            data: {
                all_affected_rows: result.all_affected_rows,
                affected_row_indices: group.affected_row_indices,
            },
        };

        console.log(`[Broadcast] Broadcasting to group with ${group.session_ids.length} sessions, ${group.affected_row_indices.length} affected row indices`);

        for (const sessionId of group.session_ids) {
            const client = connectedClients.get(sessionId);
            if (client && client.ws.readyState === 1) { // WebSocket.OPEN = 1
                console.log(`[Broadcast] Sending delta to session ${sessionId}`);
                client.ws.send(JSON.stringify(deltaMessage));
            } else {
                console.log(`[Broadcast] Skipping session ${sessionId} - client not found or websocket not open (readyState: ${client?.ws?.readyState})`);
            }
        }
    }

    console.log(`[Broadcast] Broadcast complete`);
}

// Routes
app.get("/", (c) => {
    return c.text(`Sync Playground Server 🔥 Pyre 🔥\n\nDatabase: ${DB_PATH}`);
});

// CORS middleware
app.use("*", async (c, next) => {
    c.header("Access-Control-Allow-Origin", "*");
    c.header("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
    c.header("Access-Control-Allow-Headers", "Content-Type");
    if (c.req.method === "OPTIONS") {
        return new Response(null, {
            status: 204,
            headers: {
                "Access-Control-Allow-Origin": "*",
                "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
                "Access-Control-Allow-Headers": "Content-Type",
            },
        });
    }
    await next();
});

// Query metadata endpoint
app.get("/queries", async (c) => {
    try {
        const { discoverQueries } = await import("./client/queryDiscovery.js");
        const queries = discoverQueries();
        return c.json(queries);
    } catch (error: any) {
        console.error("Failed to discover queries:", error);
        c.status(500);
        return c.json({ error: error.message });
    }
});

// Sync endpoint for initial catchup and incremental sync
app.get("/sync", async (c) => {
    try {
        // Get sessionId from query params
        const sessionId = c.req.query("sessionId");
        if (!sessionId) {
            c.status(400);
            return c.json({ error: "sessionId query parameter is required" });
        }

        // Look up client session from connected clients
        const client = connectedClients.get(sessionId);
        if (!client) {
            c.status(404);
            return c.json({ error: "Session not found. Client must be connected via WebSocket first." });
        }

        // Get syncCursor from query params (optional, defaults to empty)
        const syncCursorParam = c.req.query("syncCursor");
        let syncCursor: any = { tables: {} };
        if (syncCursorParam) {
            try {
                syncCursor = JSON.parse(syncCursorParam);
            } catch (e) {
                c.status(400);
                return c.json({ error: "Invalid syncCursor format. Must be valid JSON." });
            }
        }

        const db = createClient({
            url: `file:${DB_PATH}`,
        });

        const pageSize = 1000; // Large page size for catchup

        // Convert session to format expected by WASM
        const session = {
            fields: client.session.fields,
        };

        // Step 1: Get sync status SQL
        const statusSql = get_sync_status_sql(syncCursor, session);

        if (typeof statusSql === "string" && statusSql.startsWith("Error:")) {
            c.status(500);
            return c.json({ error: statusSql });
        }

        // Step 2: Execute sync status SQL
        const statusResult = await db.execute(statusSql as string);

        // Step 3: Get sync SQL for tables that need syncing
        const syncSqlResult = get_sync_sql(statusResult.rows, syncCursor, session, pageSize);

        if (typeof syncSqlResult === "string" && syncSqlResult.startsWith("Error:")) {
            c.status(500);
            return c.json({ error: syncSqlResult });
        }

        const sqlResult =
            typeof syncSqlResult === "string"
                ? JSON.parse(syncSqlResult)
                : syncSqlResult;

        const result: {
            tables: Record<
                string,
                {
                    rows: any[];
                    permission_hash: string;
                    last_seen_updated_at: number | null;
                }
            >;
            has_more: boolean;
        } = {
            tables: {},
            has_more: false,
        };

        // Collect all SQL statements from all tables for batch execution
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

            // Process all query results for this table
            for (const sql of tableSql.sql) {
                const queryResult = allQueryResults[resultIndex++];
                const columns = queryResult.columns;
                const rows = queryResult.rows || [];

                // Convert rows to objects (not positional arrays)
                for (const row of rows) {
                    const rowObject: Record<string, any> = {};
                    for (const column of columns) {
                        rowObject[column] = row[column];
                    }
                    tableRows.push(rowObject);

                    // Track max updatedAt
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

            // Check if there's more data (SQL fetches pageSize + 1)
            const hasMoreForTable = tableRows.length > pageSize;
            const finalRows = hasMoreForTable ? tableRows.slice(0, pageSize) : tableRows;

            // Recalculate maxUpdatedAt from returned rows if we sliced
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

            // Store results
            result.tables[tableSql.table_name] = {
                rows: finalRows,
                permission_hash: tableSql.permission_hash,
                last_seen_updated_at: maxUpdatedAt,
            };

            if (hasMoreForTable) {
                result.has_more = true;
            }
        }

        return c.json(result);
    } catch (error: any) {
        console.error("[Sync] Sync error:", error);
        console.error("[Sync] Error stack:", error.stack);
        c.status(500);
        return c.json({ error: error.message || "Internal server error" });
    }
});

app.post("/db/:req", async (c) => {
    const { req } = c.req.param();
    const args = await c.req.json();
    const sessionId = c.req.query("sessionId");

    console.log(`[Query] Received request for query ID: ${req}`);
    console.log(`[Query] Args:`, JSON.stringify(args, null, 2));
    console.log(`[Query] SessionId:`, sessionId);

    try {
        // Get Query module (cached after first import)
        console.log(`[Query] Loading query module...`);
        const Query = await getQueryModule();
        console.log(`[Query] Query module loaded successfully`);

        const env = {
            url: `file:${DB_PATH}`,
            authToken: undefined,
        };

        // Get session from connected client if sessionId provided, otherwise use default
        let session: { userId: number; role: string };
        if (sessionId) {
            const client = connectedClients.get(sessionId);
            if (client) {
                // Convert session fields to the format expected by queries
                session = {
                    userId: client.session.fields.userId as number,
                    role: client.session.fields.role as string,
                };
                console.log(`[Query] Using session from connected client:`, JSON.stringify(session, null, 2));
            } else {
                console.log(`[Query] SessionId ${sessionId} not found, using default session`);
                session = {
                    userId: 1,
                    role: "user",
                };
            }
        } else {
            console.log(`[Query] No sessionId provided, using default session`);
            session = {
                userId: 1,
                role: "user",
            };
        }

        console.log(`[Query] Executing query with session:`, JSON.stringify(session, null, 2));
        const result = await Query.run(env, req, session, args);
        console.log(`[Query] Query execution completed. Result kind: ${result.kind}`);

        if (result.kind === "success") {
            console.log(`[Query] Query succeeded. Data keys:`, Object.keys(result.data || {}));
            console.log(`[Query] Full result.data:`, JSON.stringify(result.data, null, 2));
            // Check if this was a mutation (has affected rows)
            const affectedRows = extractAffectedRows(result.data);
            console.log(`[Query] Extracted affected rows:`, JSON.stringify(affectedRows, null, 2));
            console.log(`[Query] Affected rows count:`, affectedRows.length);
            console.log(`[Query] Connected clients:`, connectedClients.size);
            if (affectedRows.length > 0) {
                console.log(`[Query] Mutation detected. Affected rows: ${affectedRows.length}`);
                // Broadcast sync deltas
                await broadcastSyncDeltas(affectedRows);
            } else {
                console.log(`[Query] No affected rows found - this might not be a mutation or _affectedRows is missing`);
            }

            return c.json(result.data);
        }

        console.error(`[Query] Query failed. Error type: ${result.errorType}, Message: ${result.message || "(empty)"}`);
        console.error(result)
        c.status(500);
        return c.json({ error: result.message || "Query execution failed" });
    } catch (error: any) {
        console.error("[Query] Query execution error:", error);
        console.error("[Query] Error stack:", error.stack);
        c.status(500);
        return c.json({ error: error.message || "Internal server error" });
    }
});

// Start server function
export default async function startServer() {
    // Load schema into WASM cache for sync deltas
    await loadSchema();

    const port = 3000;

    // Bun server with WebSocket support
    const server = Bun.serve({
        port,
        fetch: async (req) => {
            // Handle WebSocket upgrade
            if (req.url.endsWith("/sync") && req.headers.get("upgrade") === "websocket") {
                const success = server.upgrade(req);
                if (success) {
                    return undefined as any;
                }
                return new Response("WebSocket upgrade failed", { status: 500 });
            }

            // Handle regular HTTP requests
            return app.fetch(req);
        },
        websocket: {
            message: (ws, message) => {
                // Handle ping/pong or other messages if needed
                try {
                    const data = typeof message === "string" ? message : message.toString();
                    const parsed = JSON.parse(data);
                    if (parsed.type === "ping") {
                        ws.send(JSON.stringify({ type: "pong" }));
                    }
                } catch (e) {
                    // Ignore invalid JSON
                }
            },
            open: (ws) => {
                // Generate session ID and values
                const sessionId = `session_${nextSessionId++}`;
                const userId = Math.floor(Math.random() * 100) + 1; // Random userId 1-100
                const session = {
                    fields: {
                        userId: userId,
                        role: "user",
                    },
                };

                const client: ConnectedClient = {
                    sessionId,
                    session,
                    ws,
                };

                connectedClients.set(sessionId, client);
                console.log(`Client connected: ${sessionId} (userId: ${userId})`);

                ws.send(JSON.stringify({
                    type: "connected",
                    sessionId,
                    session: {
                        userId: userId,
                        role: "user",
                    },
                }));
            },
            close: (ws) => {
                // Find and remove client
                for (const [sessionId, client] of connectedClients.entries()) {
                    if (client.ws === ws) {
                        connectedClients.delete(sessionId);
                        console.log(`Client disconnected: ${sessionId}`);
                        break;
                    }
                }
            },
        },
    });

    console.log(`Server starting on http://localhost:${port}`);
    console.log(`WebSocket endpoint: ws://localhost:${port}/sync`);

    return server;
}

// If running directly (not imported), start the server
if (import.meta.main) {
    startServer().catch((error) => {
        console.error("Failed to start server:", error);
        process.exit(1);
    });
}
