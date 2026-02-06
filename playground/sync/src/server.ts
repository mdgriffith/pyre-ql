import { Hono } from "hono";
import { streamSSE } from "hono/streaming";
import { createClient } from "@libsql/client";
import { join } from "path";
import type { ServerWebSocket, WebSocketHandler } from "bun";
import * as Sync from "@pyre/server/sync";

await Sync.init();

const app = new Hono();

// Types
type LiveSyncTransport = "sse" | "websocket";

interface WebSocketData {
    sessionId: string;
    session: Record<string, any>;
}

interface ConnectedClient {
    sessionId: string;
    session: Record<string, any>;
    transport: LiveSyncTransport;
    writeSSE?: (data: { event?: string; data: any; id?: string }) => Promise<void>;
    ws?: ServerWebSocket<WebSocketData>;
}

const DB_PATH = join(process.cwd(), "test.db");
const DB_URL = `file:${DB_PATH}`;
const connectedClients = new Map<string, ConnectedClient>();
const sessionsById = new Map<string, Record<string, any>>();
let nextSessionId = 1;

const db = createClient({ url: DB_URL });

// Import query map - loaded once at startup
const queryModule = await import(join(process.cwd(), "pyre", "generated", "typescript", "targets", "server", "queries"));
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

// Session lookup helper
const getSession = (sessionId: string | undefined | null) => {
    if (!sessionId) {
        return null;
    }
    return sessionsById.get(sessionId) || null;
};

// Query metadata endpoint
app.get("/queries", async (c) => {
    const { discoverQueries } = await import("./client/queryDiscovery");
    const queries = discoverQueries();
    return c.json(queries);

});

// Login endpoint - creates a session for a userId
app.get("/login", async (c) => {
    const userId = c.req.query("userId");
    if (!userId) {
        c.status(400);
        return c.json({ error: "userId query parameter is required" });
    }

    const parsedUserId = parseInt(userId, 10);
    if (Number.isNaN(parsedUserId) || parsedUserId < 1) {
        c.status(400);
        return c.json({ error: "userId must be a positive integer" });
    }

    const sessionId = `session_${nextSessionId++}`;
    const session = {
        userId: parsedUserId,
        role: "user",
    };
    sessionsById.set(sessionId, session);
    return c.json({ sessionId });
});

// SSE endpoint for real-time delta updates
app.get("/sync/events", async (c) => {
    const sessionId = c.req.query("sessionId");

    if (!sessionId) {
        c.status(400);
        return c.json({ error: "sessionId query parameter is required" });
    }

    const session = getSession(sessionId);
    if (!session) {
        c.status(404);
        return c.json({ error: "Session not found" });
    }

    const isReconnection = connectedClients.has(sessionId);
    if (isReconnection) {
        console.log(`[RECONNECT] Client reconnected via SSE: ${sessionId}`);
    } else {
        console.log(`[NEW] Client connected via SSE: ${sessionId}`);
    }

    return streamSSE(c, async (stream) => {
        const client: ConnectedClient = {
            sessionId: sessionId!,
            session,
            transport: "sse",
            writeSSE: async (data) => {
                await stream.writeSSE(data);
            },
        };

        // Update or set the client (in case of reconnection, update the writeSSE function)
        connectedClients.set(sessionId!, client);

        // Only send "connected" message on initial connection, not reconnection
        if (!isReconnection) {
            await stream.writeSSE({
                event: "connected",
                data: JSON.stringify({
                    type: "connected",
                    sessionId,
                    session,
                }),
            });
        }

        // Keep connection alive and handle disconnect
        // The stream will be closed when the client disconnects
        try {
            // Send periodic keep-alive comments to prevent proxy/timeout issues
            // Many proxies timeout idle connections after 30-60 seconds
            // Sending a comment every 15 seconds keeps the connection alive more reliably
            let keepAliveCount = 0;
            while (true) {
                await stream.sleep(15000); // Every 15 seconds (more frequent to prevent timeouts)
                keepAliveCount++;
                // Send SSE comment (starts with ':') - this is a keep-alive that doesn't trigger events
                // Format: ": comment\n\n" (two newlines required for SSE)
                try {
                    await stream.writeSSE({ data: ": keep-alive\n\n" });
                    if (keepAliveCount % 4 === 0) { // Log every minute
                        console.log(`SSE keep-alive sent for ${sessionId} (count: ${keepAliveCount})`);
                    }
                } catch (writeError) {
                    // If write fails, connection is likely closed
                    console.error(`Failed to send keep-alive for ${sessionId}:`, writeError);
                    throw writeError; // Break the loop
                }
            }
        } catch (error) {
            // Client disconnected or stream closed
            console.log(`SSE stream ended for ${sessionId}:`, error instanceof Error ? error.message : String(error));
            // Don't delete the session immediately - EventSource will reconnect
            // Only remove if the client is explicitly disconnected
            const currentClient = connectedClients.get(sessionId!);
            if (currentClient === client) {
                // Only remove if this is still the active client (not replaced by reconnection)
                connectedClients.delete(sessionId!);
                console.log(`Client session removed: ${sessionId}`);
            }
        }
    });
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
    const session = client?.session ?? getSession(sessionId);
    if (!session) {
        c.status(404);
        return c.json({ error: "Session not found." });
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
    const result = await Sync.catchup(db, syncCursor, session, 1000);
    return c.json(result);

});


app.post("/db/:req", async (c) => {
    const { req } = c.req.param();
    const args = await c.req.json();
    const sessionId = c.req.query("sessionId");
    if (!sessionId) {
        c.status(400);
        return c.json({ error: "sessionId query parameter is required" });
    }

    // Get executing session from connected client or use default
    const client = connectedClients.get(sessionId);
    const executingSession = client?.session ?? getSession(sessionId);
    if (!executingSession) {
        c.status(404);
        return c.json({ error: "Session not found." });
    }

    // Execute query with all connected clients for sync delta calculation
    // Pyre.run can extract session.fields from ConnectedClient objects
    const result = await Sync.run(
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
    result.sync(async (sessionId, message) => {
        const client = connectedClients.get(sessionId);
        if (!client) {
            return;
        }

        const payload = {
            type: "delta",
            data: (message as { data?: any }).data ?? message,
        };

        try {
            if (client.transport === "sse" && client.writeSSE) {
                // Send SSE event with delta data
                await client.writeSSE({
                    event: "delta",
                    data: JSON.stringify(message),
                });
            } else if (client.transport === "websocket" && client.ws) {
                // ServerWebSocket readyState: 0=CONNECTING, 1=OPEN, 2=CLOSING, 3=CLOSED
                if (client.ws.readyState === 1) {
                    client.ws.send(JSON.stringify(payload));
                } else {
                    connectedClients.delete(sessionId);
                }
            }
        } catch (error) {
            console.error(`Failed to send delta to client ${sessionId}:`, error);
            connectedClients.delete(sessionId);
        }
    })

    // Return response immediately
    return c.json(result.response);

});

// Start server function
export default async function startServer() {
    // Load schema into WASM cache for sync deltas
    await Sync.loadSchemaFromDatabase(db);

    const port = 3000;

    // Bun server with SSE support
    // Production considerations:
    // - idleTimeout: Bun's maximum is 255s (~4.25 minutes) - this is sufficient with keep-alive
    // - Keep-alive messages every 15s prevent proxy timeouts (most proxies timeout at 60-120s)
    // - Reverse proxies (nginx, cloudflare) may need additional timeout configuration
    // - The keep-alive ensures connections stay alive even if idleTimeout triggers
    const SSE_IDLE_TIMEOUT = 255; // Bun's maximum - ~4.25 minutes (sufficient with 15s keep-alive)
    
    const server = Bun.serve({
        port,
        fetch: (request, server) => {
            const url = new URL(request.url);
            if (url.pathname === "/sync/events" && request.headers.get("upgrade") === "websocket") {
                const sessionId = url.searchParams.get("sessionId");
                if (!sessionId) {
                    return new Response("sessionId query parameter is required", { status: 400 });
                }
                const session = getSession(sessionId);
                if (!session) {
                    return new Response("Session not found", { status: 404 });
                }

                const upgraded = server.upgrade(request, {
                    data: { sessionId, session },
                });

                if (upgraded) {
                    return;
                }

                return new Response("WebSocket upgrade failed", { status: 400 });
            }

            return app.fetch(request);
        },
        websocket: {
            open(ws: ServerWebSocket<WebSocketData>) {
                const { sessionId, session } = ws.data;
                const isReconnection = connectedClients.has(sessionId);
                if (isReconnection) {
                    console.log(`[RECONNECT] Client reconnected via WebSocket: ${sessionId}`);
                } else {
                    console.log(`[NEW] Client connected via WebSocket: ${sessionId}`);
                }

                const client: ConnectedClient = {
                    sessionId,
                    session,
                    transport: "websocket",
                    ws,
                };
                connectedClients.set(sessionId, client);

                if (!isReconnection) {
                    ws.send(
                        JSON.stringify({
                            type: "connected",
                            sessionId,
                            session,
                        })
                    );
                }
            },
            message(ws: ServerWebSocket<WebSocketData>, message: string | Buffer) {
                // Handle incoming WebSocket messages if needed
                // Currently, this is a one-way sync (server -> client), so we don't need to handle messages
                console.log(`Received message from ${ws.data.sessionId}:`, message);
            },
            close(ws: ServerWebSocket<WebSocketData>) {
                const { sessionId } = ws.data;
                const currentClient = connectedClients.get(sessionId);
                if (currentClient?.ws === ws) {
                    connectedClients.delete(sessionId);
                    console.log(`WebSocket client session removed: ${sessionId}`);
                }
            },
        } as WebSocketHandler<WebSocketData>,
        idleTimeout: SSE_IDLE_TIMEOUT, // Bun's maximum - keep-alive prevents actual disconnection
    });

    console.log(`Server starting on http://localhost:${port}`);
    console.log(`SSE endpoint: http://localhost:${port}/sync/events`);
    console.log(`WebSocket endpoint: ws://localhost:${port}/sync/events`);
    console.log(`SSE idleTimeout: ${SSE_IDLE_TIMEOUT}s (~4.25 minutes, Bun's maximum)`);
    console.log(`Keep-alive interval: 15s (prevents idle timeout and proxy timeouts)`);

    return server;
}

// If running directly (not imported), start the server
if (import.meta.main) {
    startServer().catch((error) => {
        console.error("Failed to start server:", error);
        process.exit(1);
    });
}
