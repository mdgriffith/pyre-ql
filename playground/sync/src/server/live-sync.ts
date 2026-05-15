import type { Context } from "hono";
import { streamSSE } from "hono/streaming";
import { connectSSEClient, disconnectClient } from "./connections";
import { readPyreRequestSession, withValidation } from "./session";

// Owns the browser live-sync transport endpoint. It validates the request,
// registers the SSE connection, sends the initial connected message, and keeps
// the stream alive. Broadcast/delta delivery happens in connections.sendDelta.

export function handleLiveSyncEvents(c: Context) {
    return withValidation(c, () => {
        const { databaseId, sessionId, session } = readPyreRequestSession(c);

        return streamSSE(c, async (stream) => {
            const { client, isReconnection } = connectSSEClient({ databaseId, sessionId, session, stream });
            console.log(`${isReconnection ? "[RECONNECT]" : "[NEW]"} Client connected via SSE: ${sessionId}`);

            if (!isReconnection) {
                await stream.writeSSE({
                    data: JSON.stringify({
                        type: "connected",
                        databaseId,
                        connectionId: sessionId,
                        sessionId,
                        session,
                    }),
                });
            }

            try {
                let keepAliveCount = 0;
                while (true) {
                    await stream.sleep(15000);
                    keepAliveCount++;
                    await stream.writeSSE({ data: ": keep-alive\n\n" });
                    if (keepAliveCount % 4 === 0) {
                        console.log(`SSE keep-alive sent for ${sessionId} (count: ${keepAliveCount})`);
                    }
                }
            } catch (error) {
                console.log(`SSE stream ended for ${sessionId}:`, error instanceof Error ? error.message : String(error));
                disconnectClient(databaseId, sessionId, client);
                console.log(`Client session removed: ${sessionId}`);
            }
        });
    });
}
