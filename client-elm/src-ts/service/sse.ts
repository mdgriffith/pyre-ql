import type { ElmApp } from '../types';

export interface SSEConfig {
  baseUrl: string;
  eventsPath: string;
}

export interface LiveSyncMessage {
  type: string;
  sessionId?: string;
  data?: unknown;
  error?: string;
}

export class SSEManager {
  private eventSource: EventSource | null = null;
  private sessionId: string | null = null;
  private config: SSEConfig | null = null;
  private shouldReconnect = true;
  private onMessage: ((message: LiveSyncMessage) => void) | null = null;
  private elmApp: ElmApp | null = null;

  constructor(config: SSEConfig, onMessage?: (message: LiveSyncMessage) => void) {
    this.config = config;
    this.onMessage = onMessage ?? null;
  }

  setOnMessage(callback: (message: LiveSyncMessage) => void): void {
    this.onMessage = callback;
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

  connect(): void {
    this.shouldReconnect = true;
    this.attemptConnect();
  }

  private emitMessage(message: LiveSyncMessage): void {
    this.onMessage?.(message);
    this.elmApp?.ports.receiveSSEMessage?.send(message);
    console.log('[PyreClient] port receiveSSEMessage ->', message);
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
            const connectedMessage = {
              type: 'connected',
              sessionId: message.sessionId,
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
          const message = JSON.parse(event.data) as { data?: unknown };
          const deltaMessage = {
            type: 'delta',
            data: message.data,
          };
          this.emitMessage(deltaMessage);
        } catch (error) {
          console.error('Failed to parse SSE delta message:', error);
        }
      });

      eventSource.addEventListener('syncProgress', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data) as { data?: unknown };
          const progressMessage = {
            type: 'syncProgress',
            data: message.data,
          };
          this.emitMessage(progressMessage);
        } catch (error) {
          console.error('Failed to parse SSE sync progress message:', error);
        }
      });

      eventSource.addEventListener('syncComplete', () => {
        const completeMessage = {
          type: 'syncComplete',
        };
        this.emitMessage(completeMessage);
      });

      eventSource.onerror = () => {
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

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.sessionId = null;
  }
}
