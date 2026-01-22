import type { ElmApp, SSEConfig, SyncProgress } from '../types';

type SyncProgressCallback = (progress: SyncProgress) => void;
type SessionCallback = (sessionId: string) => void;

export class SSEManager {
  private eventSource: EventSource | null = null;
  private sessionId: string | null = null;
  private config: SSEConfig | null = null;
  private elmApp: ElmApp | null = null;
  private clientId: string;
  private shouldReconnect = true;
  private syncProgressCallbacks: Set<SyncProgressCallback> = new Set();
  private sessionCallbacks: Set<SessionCallback> = new Set();
  private lastSyncProgress: SyncProgress | null = null;

  constructor() {
    this.clientId = `client_${Math.random().toString(36).substring(2, 15)}_${Date.now()}`;
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.sseOut) {
      elmApp.ports.sseOut.subscribe((message) => {
        const typedMessage = message as { type?: string; config?: SSEConfig };
        if (typedMessage.type === 'connectSSE' && typedMessage.config) {
          this.connect(typedMessage.config);
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

  connect(config: SSEConfig): void {
    this.config = config;
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
      const sseUrl = `${this.config.baseUrl}/sync/events?userId=${this.config.userId}&clientId=${encodeURIComponent(this.clientId)}`;
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
            this.elmApp?.ports.receiveSSEMessage?.send({
              type: 'connected',
              sessionId: message.sessionId,
            });
          }
        } catch (error) {
          console.error('Failed to parse SSE connected message:', error);
          this.elmApp?.ports.receiveSSEMessage?.send({
            type: 'error',
            error: 'Failed to parse connection message',
          });
        }
      });

      eventSource.addEventListener('delta', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { data?: unknown };
          this.elmApp?.ports.receiveSSEMessage?.send({
            type: 'delta',
            data: message.data,
          });
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
          this.elmApp?.ports.receiveSSEMessage?.send({
            type: 'syncProgress',
            data: message.data,
          });
        } catch (error) {
          console.error('Failed to parse SSE sync progress message:', error);
        }
      });

      eventSource.addEventListener('syncComplete', () => {
        if (this.lastSyncProgress) {
          this.emitSyncProgress({ ...this.lastSyncProgress, complete: true });
        }
        this.elmApp?.ports.receiveSSEMessage?.send({
          type: 'syncComplete',
        });
      });

      eventSource.onerror = (error) => {
        const state = eventSource.readyState;

        if (state === EventSource.CLOSED) {
          console.warn('[PyreClient] SSE connection closed');
          if (this.shouldReconnect) {
            // EventSource will auto-reconnect
          }
        } else if (state === EventSource.CONNECTING && !this.sessionId) {
          this.elmApp?.ports.receiveSSEMessage?.send({
            type: 'error',
            error: 'SSE connection failed',
          });
        }
      };
    } catch (error) {
      this.elmApp?.ports.receiveSSEMessage?.send({
        type: 'error',
        error: `SSE connection error: ${error}`,
      });
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
