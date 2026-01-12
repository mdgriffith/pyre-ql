import { Hono } from "hono";
import { createClient } from "@libsql/client";
import { join } from "path";
import * as Pyre from "../../../wasm/server";

await Pyre.init();

const app = new Hono();

// Types
interface WebSocketData {
    userId: string;
}

interface ConnectedClient {
    sessionId: string;
    session: Record<string, any>;
    ws: any; // Bun WebSocket
}

const DB_PATH = join(process.cwd(), "test.db");
const DB_URL = `file:${DB_PATH}`;
const connectedClients = new Map<string, ConnectedClient>();
let nextSessionId = 1;

const db = createClient({ url: DB_URL });

// Import query map - loaded once at startup
const queryModule = await import(join(process.cwd(), "pyre", "generated", "server", "typescript", "query"));
const queries = queryModule.queries;

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
    const { discoverQueries } = await import("./client/queryDiscovery.js");
    const queries = discoverQueries();
    return c.json(queries);

});

// Schema endpoint - returns introspection JSON
app.get("/schema", async (c) => {
    const { getIntrospectionJson } = await import("../../../wasm/server/schema.js");
    const introspection = await getIntrospectionJson(db);
    return c.json(introspection);
});

// Sync endpoint - Much simpler with helpers!
app.get("/sync", async (c) => {

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

    // Ask pyre to get any data that needs to be synced.
    const result = await Pyre.catchup(db, syncCursor, client.session, 1000);
    return c.json(result);

});


app.post("/db/:req", async (c) => {
    const { req } = c.req.param();
    const args = await c.req.json();
    const sessionId = c.req.query("sessionId");

    // Get executing session from connected client or use default
    const client = sessionId ? connectedClients.get(sessionId) : null;
    const executingSession = client?.session ?? { userId: 1, role: "user" };

    // Execute query with all connected clients for sync delta calculation
    // Pyre.run can extract session.fields from ConnectedClient objects
    const result = await Pyre.run(
        db,
        queries,
        req,
        args,
        executingSession,
        connectedClients
    );

    if (result.kind === "error") {
        c.status(500);
        return c.json({ error: result.error?.message || "Query execution failed" });
    }

    // Broadcast sync deltas in background (fire-and-forget, we're not awaiting it)
    result.sync((sessionId, message) => {
        const client = connectedClients.get(sessionId);
        if (client && client.ws.readyState === 1) {
            client.ws.send(JSON.stringify(message));
        }
    })

    // Return response immediately
    return c.json(result.response);

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
                    userId: userId,
                    role: "user",
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
                    session,
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
