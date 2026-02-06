import type { LiveSyncMessage } from './sse';

export interface WebSocketConfig {
  baseUrl: string;
  eventsPath: string;
  reconnectDelayMs?: number;
}

import type { ElmApp } from '../types';

export class WebSocketManager {
  private socket: WebSocket | null = null;
  private config: WebSocketConfig;
  private shouldReconnect = true;
  private reconnectTimer: number | null = null;
  private onMessage: ((message: LiveSyncMessage) => void) | null = null;
  private elmApp: ElmApp | null = null;

  constructor(config: WebSocketConfig, onMessage?: (message: LiveSyncMessage) => void) {
    this.config = config;
    this.onMessage = onMessage ?? null;
  }

  setOnMessage(callback: (message: LiveSyncMessage) => void): void {
    this.onMessage = callback;
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.webSocketOut) {
      elmApp.ports.webSocketOut.subscribe((message) => {
        console.log('[PyreClient] port webSocketOut <-', message);
        const typedMessage = message as { type?: string };
        if (typedMessage.type === 'connectWebSocket') {
          this.connect();
        } else if (typedMessage.type === 'disconnectWebSocket') {
          this.disconnect();
        }
      });
    }
  }

  connect(): void {
    this.shouldReconnect = true;
    this.openSocket();
  }

  private emitMessage(message: LiveSyncMessage): void {
    this.onMessage?.(message);
    this.elmApp?.ports.receiveWebSocketMessage?.send(message);
    console.log('[PyreClient] port receiveWebSocketMessage ->', message);
  }

  private openSocket(): void {
    const wsUrl = this.buildWebSocketUrl();
    const socket = new WebSocket(wsUrl);

    socket.onopen = () => {
      this.socket = socket;
    };

    socket.onmessage = (event) => {
      if (typeof event.data !== 'string') {
        return;
      }
      try {
        const message = JSON.parse(event.data) as LiveSyncMessage;
        this.emitMessage(message);
      } catch (error) {
        console.error('[PyreClient] Failed to parse WebSocket message:', error);
        const errorMessage = {
          type: 'error',
          error: 'Failed to parse WebSocket message',
        };
        this.emitMessage(errorMessage);
      }
    };

    socket.onerror = () => {
      const errorMessage = {
        type: 'error',
        error: 'WebSocket connection error',
      };
      this.emitMessage(errorMessage);
    };

    socket.onclose = () => {
      this.socket = null;
      if (!this.shouldReconnect) {
        return;
      }
      if (this.reconnectTimer !== null) {
        return;
      }
      const delay = this.config.reconnectDelayMs ?? 1000;
      this.reconnectTimer = window.setTimeout(() => {
        this.reconnectTimer = null;
        if (this.shouldReconnect) {
          this.openSocket();
        }
      }, delay);
    };
  }

  private buildWebSocketUrl(): string {
    const url = new URL(this.config.eventsPath, this.config.baseUrl);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    return url.toString();
  }

  disconnect(): void {
    this.shouldReconnect = false;
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket) {
      this.socket.close();
      this.socket = null;
    }
  }
}
