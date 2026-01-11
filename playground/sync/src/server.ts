import { Hono } from "hono";
import { createClient } from "@libsql/client";
import { join } from "path";
import * as Pyre from "../../../wasm/server";

await Pyre.initPyre();

const app = new Hono();

// Types
interface WebSocketData {
    userId: string;
}

interface ConnectedClient {
    sessionId: string;
    session: {
        fields: Record<string, any>;
    };
    ws: any; // Bun WebSocket
}

const DB_PATH = join(process.cwd(), "test.db");
const DB_URL = `file:${DB_PATH}`;
const QUERY_MODULE_PATH = join(process.cwd(), "pyre", "generated", "server", "typescript", "query");
const connectedClients = new Map<string, ConnectedClient>();
let nextSessionId = 1;

const db = createClient({ url: DB_URL });

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

// Sync endpoint - Much simpler with helpers!
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

        // Use the sync handler - all the complex logic is abstracted!
        const result = await Pyre.handleSync(db, syncCursor, client.session, 1000);
        return c.json(result);
    } catch (error: any) {
        console.error("[Sync] Sync error:", error);
        console.error("[Sync] Error stack:", error.stack);
        c.status(500);
        return c.json({ error: error.message || "Internal server error" });
    }
});

// Query endpoint - Much simpler with helpers!
app.post("/db/:req", async (c) => {
    const { req } = c.req.param();
    const args = await c.req.json();
    const sessionId = c.req.query("sessionId");

    try {
        // Get executing session from connected client or use default
        let executingSession: { userId: number; role: string };
        if (sessionId) {
            const client = connectedClients.get(sessionId);
            if (client) {
                executingSession = {
                    userId: client.session.fields.userId as number,
                    role: client.session.fields.role as string,
                };
            } else {
                executingSession = { userId: 1, role: "user" };
            }
        } else {
            executingSession = { userId: 1, role: "user" };
        }

        // Transform connectedClients to the format expected by runQuery
        const connectedSessionsMap = new Map(
            Array.from(connectedClients.entries()).map(([sessionId, client]) => [
                sessionId,
                { fields: client.session.fields as Record<string, any> },
            ])
        );

        // Execute query with all connected sessions for sync delta calculation
        const result = await Pyre.runQuery(
            db,
            QUERY_MODULE_PATH,
            DB_URL,
            req,
            args,
            executingSession,
            connectedSessionsMap
        );

        if (result.kind === "error") {
            c.status(500);
            return c.json({ error: result.error?.message || "Query execution failed" });
        }

        // Broadcast sync deltas in background (fire-and-forget)
        result.syncDeltas.sync((sessionId, message) => {
            const client = connectedClients.get(sessionId);
            if (client && client.ws.readyState === 1) {
                client.ws.send(JSON.stringify(message));
            }
        }).catch((error) => {
            // Log errors but don't block response
            console.error("[SyncDeltas] Error broadcasting:", error);
        });

        // Return response immediately
        return c.json(result.response);
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
    await Pyre.loadSchemaFromDatabase(db);

    const port = 3000;

    // Bun server with WebSocket support
    const server = Bun.serve({
        port,
        fetch: async (req) => {
            // Handle WebSocket upgrade
            const url = new URL(req.url);
            if (url.pathname === "/sync" && req.headers.get("upgrade") === "websocket") {
                const userId = url.searchParams.get("userId");
                if (!userId) {
                    return new Response("userId query parameter is required", { status: 400 });
                }
                const upgradeData: WebSocketData = {
                    userId: userId,
                };
                const success = server.upgrade(req, {
                    // @ts-expect-error - Bun's types don't properly support the data property, but it works at runtime
                    data: upgradeData,
                });
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
                const data = typeof message === "string" ? message : message.toString();
                const parsed = JSON.parse(data);
                if (parsed.type === "ping") {
                    ws.send(JSON.stringify({ type: "pong" }));
                }

            },
            open: (ws) => {
                // Get userId from upgrade data (passed via query parameter)
                // @ts-expect-error - Bun's types don't properly support the data property, but it works at runtime
                const userIdParam = (ws.data as WebSocketData)?.userId;
                let userId: number;

                if (userIdParam) {
                    const parsedUserId = parseInt(userIdParam, 10);
                    if (isNaN(parsedUserId) || parsedUserId < 1) {
                        console.error(`Invalid userId provided: ${userIdParam}, defaulting to 1`);
                        userId = 1;
                    } else {
                        userId = parsedUserId;
                    }
                } else {
                    console.warn("No userId provided in WebSocket connection, defaulting to 1");
                    userId = 1;
                }

                // Generate session ID
                const sessionId = `session_${nextSessionId++}`;
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
