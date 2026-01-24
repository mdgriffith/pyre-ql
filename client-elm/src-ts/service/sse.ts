import type { ElmApp, SyncProgress } from '../types';

export interface SSEConfig {
  baseUrl: string;
  eventsPath: string;
}

type SyncProgressCallback = (progress: SyncProgress) => void;
type SessionCallback = (sessionId: string) => void;

export class SSEManager {
  private eventSource: EventSource | null = null;
  private sessionId: string | null = null;
  private config: SSEConfig | null = null;
  private elmApp: ElmApp | null = null;
  private shouldReconnect = true;
  private syncProgressCallbacks: Set<SyncProgressCallback> = new Set();
  private sessionCallbacks: Set<SessionCallback> = new Set();
  private lastSyncProgress: SyncProgress | null = null;

  constructor(config: SSEConfig) {
    this.config = config;
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.sseOut) {
      elmApp.ports.sseOut.subscribe((message) => {
        console.log('[PyreClient] port sseOut <-', message);
        const typedMessage = message as { type?: string };
        if (typedMessage.type === 'connectSSE') {
          this.connect();
        } else if (typedMessage.type === 'disconnectSSE') {
          this.disconnect();
        }
      });
    }
  }

  onSyncProgress(callback: SyncProgressCallback): () => void {
    this.syncProgressCallbacks.add(callback);
    if (this.lastSyncProgress) {
      callback(this.lastSyncProgress);
    }
    return () => {
      this.syncProgressCallbacks.delete(callback);
    };
  }

  onSession(callback: SessionCallback): () => void {
    this.sessionCallbacks.add(callback);
    if (this.sessionId) {
      callback(this.sessionId);
    }
    return () => {
      this.sessionCallbacks.delete(callback);
    };
  }

  getSessionId(): string | null {
    return this.sessionId;
  }

  connect(): void {
    this.shouldReconnect = true;
    this.attemptConnect();
  }

  private emitSyncProgress(progress: SyncProgress): void {
    this.lastSyncProgress = progress;
    this.syncProgressCallbacks.forEach((callback) => {
      try {
        callback(progress);
      } catch (error) {
        console.error('[PyreClient] Sync progress callback failed:', error);
      }
    });
  }

  private emitSession(sessionId: string): void {
    this.sessionCallbacks.forEach((callback) => {
      try {
        callback(sessionId);
      } catch (error) {
        console.error('[PyreClient] Session callback failed:', error);
      }
    });
  }

  private attemptConnect(): void {
    if (!this.config) {
      return;
    }

    try {
      const sseUrl = `${this.config.baseUrl}${this.config.eventsPath}`;
      const eventSource = new EventSource(sseUrl);

      eventSource.onopen = () => {
        this.eventSource = eventSource;
      };

      eventSource.addEventListener('connected', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { sessionId?: string };
          if (message.sessionId) {
            this.sessionId = message.sessionId;
            this.emitSession(message.sessionId);
            const connectedMessage = {
              type: 'connected',
              sessionId: message.sessionId,
            };
            this.elmApp?.ports.receiveSSEMessage?.send(connectedMessage);
            console.log('[PyreClient] port receiveSSEMessage ->', connectedMessage);
          }
        } catch (error) {
          console.error('Failed to parse SSE connected message:', error);
          const errorMessage = {
            type: 'error',
            error: 'Failed to parse connection message',
          };
          this.elmApp?.ports.receiveSSEMessage?.send(errorMessage);
          console.log('[PyreClient] port receiveSSEMessage ->', errorMessage);
        }
      });

      eventSource.addEventListener('delta', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { data?: unknown };
          const deltaMessage = {
            type: 'delta',
            data: message.data,
          };
          this.elmApp?.ports.receiveSSEMessage?.send(deltaMessage);
          console.log('[PyreClient] port receiveSSEMessage ->', deltaMessage);
        } catch (error) {
          console.error('Failed to parse SSE delta message:', error);
        }
      });

      eventSource.addEventListener('syncProgress', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { data?: SyncProgress };
          if (message.data) {
            this.emitSyncProgress(message.data);
          }
          const progressMessage = {
            type: 'syncProgress',
            data: message.data,
          };
          this.elmApp?.ports.receiveSSEMessage?.send(progressMessage);
          console.log('[PyreClient] port receiveSSEMessage ->', progressMessage);
        } catch (error) {
          console.error('Failed to parse SSE sync progress message:', error);
        }
      });

      eventSource.addEventListener('syncComplete', () => {
        if (this.lastSyncProgress) {
          this.emitSyncProgress({ ...this.lastSyncProgress, complete: true });
        }
        const completeMessage = {
          type: 'syncComplete',
        };
        this.elmApp?.ports.receiveSSEMessage?.send(completeMessage);
        console.log('[PyreClient] port receiveSSEMessage ->', completeMessage);
      });

      eventSource.onerror = (error) => {
        const state = eventSource.readyState;

        if (state === EventSource.CLOSED) {
          console.warn('[PyreClient] SSE connection closed');
          if (this.shouldReconnect) {
            // EventSource will auto-reconnect
          }
        } else if (state === EventSource.CONNECTING && !this.sessionId) {
          const errorMessage = {
            type: 'error',
            error: 'SSE connection failed',
          };
          this.elmApp?.ports.receiveSSEMessage?.send(errorMessage);
          console.log('[PyreClient] port receiveSSEMessage ->', errorMessage);
        }
      };
    } catch (error) {
      const errorMessage = {
        type: 'error',
        error: `SSE connection error: ${error}`,
      };
      this.elmApp?.ports.receiveSSEMessage?.send(errorMessage);
      console.log('[PyreClient] port receiveSSEMessage ->', errorMessage);
    }
  }

  disconnect(): void {
    this.shouldReconnect = false;

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.sessionId = null;
  }
}
