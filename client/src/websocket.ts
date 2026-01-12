/**
 * WebSocket connection management for Pyre Client
 */

import type { ClientConfig } from './types';

export interface WebSocketMessage {
  type: 'connected' | 'delta' | 'ping' | 'pong';
  sessionId?: string;
  session?: any;
  data?: any;
}

export class WebSocketManager {
  private config: Required<ClientConfig>;
  private ws: WebSocket | null = null;
  private reconnectTimer: number | null = null;
  private reconnectAttempts = 0;
  private sessionId: string | null = null;
  private onMessageCallback: ((message: WebSocketMessage) => void) | null = null;
  private onConnectCallback: ((sessionId: string) => void) | null = null;
  private onDisconnectCallback: (() => void) | null = null;
  private shouldReconnect = true;

  constructor(config: Required<ClientConfig>) {
    this.config = config;
  }

  onMessage(callback: (message: WebSocketMessage) => void) {
    this.onMessageCallback = callback;
  }

  onConnect(callback: (sessionId: string) => void) {
    this.onConnectCallback = callback;
  }

  onDisconnect(callback: () => void) {
    this.onDisconnectCallback = callback;
  }

  connect(): Promise<string> {
    return new Promise((resolve, reject) => {
      this.shouldReconnect = true;
      this.attemptConnect(resolve, reject);
    });
  }

  private attemptConnect(resolve: (sessionId: string) => void, reject: (error: Error) => void) {
    try {
      const wsUrl = this.config.baseUrl.replace(/^http/, 'ws') + `/sync?userId=${this.config.userId}`;
      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        this.ws = ws;
        this.reconnectAttempts = 0;
        
        // Send ping to keep connection alive
        this.startPingInterval();
      };

      ws.onmessage = (event) => {
        try {
          const message: WebSocketMessage = JSON.parse(event.data);
          
          if (message.type === 'connected' && message.sessionId) {
            this.sessionId = message.sessionId;
            if (this.onConnectCallback) {
              this.onConnectCallback(message.sessionId);
            }
            resolve(message.sessionId);
          } else if (message.type === 'pong') {
            // Heartbeat response, do nothing
          } else {
            if (this.onMessageCallback) {
              this.onMessageCallback(message);
            }
          }
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error);
        }
      };

      ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        if (this.reconnectAttempts === 0) {
          // Only reject on first attempt
          reject(new Error('WebSocket connection failed'));
        }
      };

      ws.onclose = () => {
        this.ws = null;
        this.sessionId = null;
        
        if (this.onDisconnectCallback) {
          this.onDisconnectCallback();
        }

        if (this.shouldReconnect) {
          this.scheduleReconnect(resolve, reject);
        }
      };
    } catch (error) {
      reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private scheduleReconnect(resolve: (sessionId: string) => void, reject: (error: Error) => void) {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
    }

    const initialDelay = this.config.reconnect.initialDelay;
    const maxDelay = this.config.reconnect.maxDelay;
    const backoffMultiplier = this.config.reconnect.backoffMultiplier;

    const delay = Math.min(
      initialDelay * Math.pow(backoffMultiplier, this.reconnectAttempts),
      maxDelay
    );

    this.reconnectAttempts++;

    this.reconnectTimer = window.setTimeout(() => {
      this.attemptConnect(resolve, reject);
    }, delay);
  }

  private startPingInterval() {
    const pingInterval = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'ping' }));
      } else {
        clearInterval(pingInterval);
      }
    }, 30000); // Ping every 30 seconds
  }

  disconnect() {
    this.shouldReconnect = false;
    
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    this.sessionId = null;
  }

  getSessionId(): string | null {
    return this.sessionId;
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }
}
