import type { ElmApp } from '../types';
import { resolveEndpointUrl, type DatabaseId } from '../routing';

export interface SSEConfig {
  baseUrl: string;
  eventsPath: string;
  credentials?: RequestCredentials;
  withCredentials?: boolean;
  databaseId?: DatabaseId;
}

export interface LiveSyncMessage {
  type: string;
  databaseId?: string;
  connectionId?: string;
  data?: unknown;
  error?: string;
}

export class SSEManager {
  private eventSource: EventSource | null = null;
  private connectionId: string | null = null;
  private config: SSEConfig | null = null;
  private shouldReconnect = true;
  private onMessage: ((message: LiveSyncMessage) => void) | null = null;
  private elmApp: ElmApp | null = null;
  private debugLog: (...args: unknown[]) => void;

  constructor(
    config: SSEConfig,
    onMessage?: (message: LiveSyncMessage) => void,
    debugLog?: (...args: unknown[]) => void
  ) {
    this.config = config;
    this.onMessage = onMessage ?? null;
    this.debugLog = debugLog ?? (() => {});
  }

  setOnMessage(callback: (message: LiveSyncMessage) => void): void {
    this.onMessage = callback;
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.sseOut) {
      elmApp.ports.sseOut.subscribe((message) => {
        this.debugLog('[PyreClient] port sseOut <-', message);
        const typedMessage = message as { type?: string };
        if (typedMessage.type === 'connectSSE') {
          this.connect();
        } else if (typedMessage.type === 'disconnectSSE') {
          this.disconnect();
        }
      });
    }
  }

  connect(): void {
    this.shouldReconnect = true;
    this.debugLog('[PyreClient] SSE connect requested');
    this.attemptConnect();
  }

  private emitMessage(message: LiveSyncMessage): void {
    this.onMessage?.(message);
    this.elmApp?.ports.receiveSSEMessage?.send(message);
    this.debugLog('[PyreClient] port receiveSSEMessage ->', message);
  }

  private attemptConnect(): void {
    if (!this.config) {
      this.debugLog('[PyreClient] SSE connect skipped: missing config');
      return;
    }

    try {
      const sseUrl = buildSSEUrl(this.config);
      this.debugLog('[PyreClient] SSE attempting connection', { sseUrl });
      const eventSource = new EventSource(sseUrl, {
        withCredentials: shouldIncludeCredentials(this.config),
      });

      eventSource.onopen = () => {
        this.eventSource = eventSource;
        this.debugLog('[PyreClient] SSE connection opened', { sseUrl });
      };

      eventSource.addEventListener('connected', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { connectionId?: string; databaseId?: string };
          const connectionId = message.connectionId;
          if (connectionId) {
            this.connectionId = connectionId;
            this.debugLog('[PyreClient] SSE connected', { connectionId });
            const connectedMessage = {
              type: 'connected',
              databaseId: message.databaseId,
              connectionId,
            };
            this.emitMessage(connectedMessage);
          }
        } catch (error) {
          console.error('Failed to parse SSE connected message:', error);
          const errorMessage = {
            type: 'error',
            error: 'Failed to parse connection message',
          };
          this.emitMessage(errorMessage);
        }
      });

      eventSource.addEventListener('delta', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { databaseId?: string; data?: unknown };
          const deltaMessage = {
            type: 'delta',
            databaseId: message.databaseId,
            data: message.data,
          };
          this.emitMessage(deltaMessage);
        } catch (error) {
          console.error('Failed to parse SSE delta message:', error);
        }
      });

      eventSource.addEventListener('syncProgress', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { databaseId?: string; data?: unknown };
          const progressMessage = {
            type: 'syncProgress',
            databaseId: message.databaseId,
            data: message.data,
          };
          this.emitMessage(progressMessage);
        } catch (error) {
          console.error('Failed to parse SSE sync progress message:', error);
        }
      });

      eventSource.addEventListener('syncComplete', (event: MessageEvent) => {
        const message = parseOptionalSSEData(event.data);
        const completeMessage = {
          type: 'syncComplete',
          databaseId: message.databaseId,
        };
        this.emitMessage(completeMessage);
      });

      eventSource.onerror = () => {
        const state = eventSource.readyState;
        this.debugLog('[PyreClient] SSE connection state changed', {
          readyState: state,
          connectionId: this.connectionId,
          shouldReconnect: this.shouldReconnect,
        });

        if (state === EventSource.CLOSED) {
          console.warn('[PyreClient] SSE connection closed');
          if (this.shouldReconnect) {
            this.debugLog('[PyreClient] SSE waiting for EventSource auto-reconnect');
          }
        } else if (state === EventSource.CONNECTING && !this.connectionId) {
          this.debugLog('[PyreClient] SSE failed before session established');
          const errorMessage = {
            type: 'error',
            error: 'SSE connection failed',
          };
          this.emitMessage(errorMessage);
        }
      };
    } catch (error) {
      const errorMessage = {
        type: 'error',
        error: `SSE connection error: ${error}`,
      };
      this.emitMessage(errorMessage);
    }
  }

  disconnect(): void {
    this.shouldReconnect = false;
    this.debugLog('[PyreClient] SSE disconnect requested', { connectionId: this.connectionId });

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.connectionId = null;
  }
}

function parseOptionalSSEData(data: unknown): { databaseId?: string } {
  if (typeof data !== 'string' || data.trim() === '') {
    return {};
  }

  try {
    return JSON.parse(data) as { databaseId?: string };
  } catch {
    return {};
  }
}

export function buildSSEUrl(config: SSEConfig): string {
  return resolveEndpointUrl(config.baseUrl, config.eventsPath, {
    databaseId: config.databaseId,
  });
}

function shouldIncludeCredentials(config: SSEConfig): boolean {
  return config.credentials === 'include' || config.withCredentials === true;
}
