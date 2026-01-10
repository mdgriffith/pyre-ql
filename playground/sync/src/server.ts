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
    if (affectedRows.length === 0 || connectedClients.size === 0) {
        return;
    }

    // Convert connected clients to format expected by calculate_sync_deltas
    const connectedSessions: ConnectedSession[] = Array.from(connectedClients.values()).map(
        (client) => ({
            session_id: client.sessionId,
            fields: client.session.fields,
        })
    );

    // Calculate sync deltas
    const deltasResult = calculate_sync_deltas(affectedRows, connectedSessions);

    if (typeof deltasResult === "string" && deltasResult.startsWith("Error:")) {
        console.error("Failed to calculate sync deltas:", deltasResult);
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
            const client = connectedClients.get(sessionId);
            if (client && client.ws.readyState === 1) { // WebSocket.OPEN = 1
                client.ws.send(JSON.stringify(deltaMessage));
            }
        }
    }
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

app.post("/db/:req", async (c) => {
    const { req } = c.req.param();
    const args = await c.req.json();

    console.log(`[Query] Received request for query ID: ${req}`);
    console.log(`[Query] Args:`, JSON.stringify(args, null, 2));

    try {
        // Get Query module (cached after first import)
        console.log(`[Query] Loading query module...`);
        const Query = await getQueryModule();
        console.log(`[Query] Query module loaded successfully`);

        const env = {
            url: `file:${DB_PATH}`,
            authToken: undefined,
        };

        // Use a default session for now (in real app, get from auth)
        const session = {
            userId: 1,
            role: "user",
        };

        console.log(`[Query] Executing query with session:`, JSON.stringify(session, null, 2));
        const result = await Query.run(env, req, session, args);
        console.log(`[Query] Query execution completed. Result kind: ${result.kind}`);

        if (result.kind === "success") {
            console.log(`[Query] Query succeeded. Data keys:`, Object.keys(result.data || {}));
            // Check if this was a mutation (has affected rows)
            const affectedRows = extractAffectedRows(result.data);
            if (affectedRows.length > 0) {
                console.log(`[Query] Mutation detected. Affected rows: ${affectedRows.length}`);
                // Broadcast sync deltas
                await broadcastSyncDeltas(affectedRows);
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

                ws.send(JSON.stringify({ type: "connected", sessionId }));
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
