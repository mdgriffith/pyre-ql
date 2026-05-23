import { Hono } from "hono";
import { createClient } from "@libsql/client";
import { join } from "path";
import * as Sync from "@pyre/server/sync";

// This file is the playground's Pyre integration map. App/server plumbing like
// session validation and SSE connection lifecycle lives under ./server so the
// Pyre-specific calls stay easy to find here.
import {
    connectionsForDatabase,
    sendPyreSyncMessage,
} from "./server/connections";
import { handleLiveSyncEvents } from "./server/live-sync";
import {
    createSession,
    readLoginRequest,
    readPyreRequestSession,
    readSyncCursor,
    withValidation,
} from "./server/session";

await Sync.init();

const app = new Hono();

const DB_PATH = join(process.cwd(), "test.db");
const DB_URL = `file:${DB_PATH}`;

const db = createClient({ url: DB_URL });

// Import query map - loaded once at startup
const queryModule = await import(join(process.cwd(), "pyre", "generated", "typescript", "server"));
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
    const { discoverQueries } = await import("./client/queryDiscovery");
    const queries = discoverQueries();
    return c.json(queries);

});

// Login endpoint - creates a session for a userId
app.get("/login", async (c) => {
    return withValidation(c, () => {
        const { userId } = readLoginRequest(c);
        const { sessionId } = createSession(userId);
        return c.json({ sessionId });
    });
});

app.get("/sync/events", handleLiveSyncEvents);

// Sync endpoint - Much simpler with helpers!
app.get("/sync", async (c) => {
    return withValidation(c, async () => {
        const request = readPyreRequestSession(c);
        const syncCursor = readSyncCursor(c);

        // Ask pyre to get any data that needs to be synced.
        const result = await Sync.catchup(db, syncCursor as any, request.session, 1000, request.databaseId);
        return c.json(result);
    });

});


app.post("/db/:req", async (c) => {
    return withValidation(c, async () => {
        const { req } = c.req.param();
        const args = await c.req.json();

        const request = readPyreRequestSession(c);

        // Execute query with all connected clients for sync delta calculation
        // Pyre.run can extract session.fields from ConnectedClient objects
        const result = await Sync.run(
            db,
            queries,
            req,
            args,
            request.session,
            connectionsForDatabase(request.databaseId),
            request.databaseId,
            request.sessionId
        );

        if (result.kind === "error") {
            c.status(500);
            return c.json({ error: result.error?.message || "Query execution failed" });
        }

        // Broadcast sync deltas before responding so mutation responses can observe sync failures.
        await result.sync(async (sessionId, message) => {
            await sendPyreSyncMessage(request.databaseId, sessionId, message);
        });

        return c.json(result.response);
    });

});

// Start server function
export default async function startServer() {
    // Load schema into WASM cache for sync deltas
    await Sync.loadSchemaFromDatabase(db);

    const port = 3000;

    const server = Bun.serve({
        port,
        fetch: (request) => {
            return app.fetch(request);
        },
        idleTimeout: 255,
    });

    console.log(`Server starting on http://localhost:${port}`);
    console.log(`SSE endpoint: http://localhost:${port}/sync/events`);
    console.log(`SSE idleTimeout: 255s (~4.25 minutes, Bun's maximum)`);
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
