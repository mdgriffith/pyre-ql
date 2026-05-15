import type { SSEStreamingApi } from "hono/streaming";
import type { Session } from "./session";

// Tracks currently connected live-sync clients and forwards already-computed
// Pyre sync messages to them. Pyre owns the message envelope shape, including
// fields like type, databaseId, and data; this file only chooses the recipient
// and writes the message as a standard SSE message event.

export interface ConnectedClient {
    databaseId: string;
    sessionId: string;
    session: Session;
    transport: "sse";
    writeSSE?: (data: { event?: string; data: any; id?: string }) => Promise<void>;
}

const connectedClients = new Map<string, ConnectedClient>();

export function connectionKey(databaseId: string, sessionId: string): string {
    return `${databaseId}:${sessionId}`;
}

export function getConnectedClient(databaseId: string, sessionId: string): ConnectedClient | undefined {
    return connectedClients.get(connectionKey(databaseId, sessionId));
}

export function connectionsForDatabase(databaseId: string): Map<string, ConnectedClient> {
    return new Map(
        Array.from(connectedClients.values())
            .filter((client) => client.databaseId === databaseId)
            .map((client) => [client.sessionId, client] as const)
    );
}

export function connectSSEClient(input: {
    databaseId: string;
    sessionId: string;
    session: Session;
    stream: SSEStreamingApi;
}): { client: ConnectedClient; isReconnection: boolean } {
    const key = connectionKey(input.databaseId, input.sessionId);
    const isReconnection = connectedClients.has(key);
    const client: ConnectedClient = {
        databaseId: input.databaseId,
        sessionId: input.sessionId,
        session: input.session,
        transport: "sse",
        writeSSE: (data) => input.stream.writeSSE(data),
    };
    connectedClients.set(key, client);
    return { client, isReconnection };
}

export function disconnectClient(databaseId: string, sessionId: string, client?: ConnectedClient): void {
    const key = connectionKey(databaseId, sessionId);
    if (!client || connectedClients.get(key) === client) {
        connectedClients.delete(key);
    }
}

export async function sendPyreSyncMessage(databaseId: string, sessionId: string, message: unknown): Promise<void> {
    const client = getConnectedClient(databaseId, sessionId);
    if (!client) {
        return;
    }

    try {
        if (client.writeSSE) {
            await client.writeSSE({
                data: JSON.stringify(message),
            });
        }
    } catch (error) {
        console.error(`Failed to send delta to client ${sessionId}:`, error);
        disconnectClient(databaseId, sessionId);
    }
}
