/**
 * Server-Sent Events (SSE) connection management for Pyre Client
 */

import type { ClientConfig } from './types';

export interface SSEMessage {
  type: 'connected' | 'delta';
  sessionId?: string;
  session?: any;
  data?: any;
}

export class SSEManager {
  private config: ClientConfig;
  private eventSource: EventSource | null = null;
  private sessionId: string | null = null;
  private onMessageCallback: ((message: SSEMessage) => void) | null = null;
  private onConnectCallback: ((sessionId: string) => void) | null = null;
  private onDisconnectCallback: (() => void) | null = null;
  private shouldReconnect = true;
  private clientId: string;

  constructor(config: ClientConfig) {
    this.config = config;
    // Generate a unique client ID for this instance (persists across reconnections)
    // This helps distinguish between multiple tabs/browsers with the same userId
    this.clientId = `client_${Math.random().toString(36).substring(2, 15)}_${Date.now()}`;
  }

  onMessage(callback: (message: SSEMessage) => void) {
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
      // SSE endpoint with userId and clientId as query parameters
      // clientId helps distinguish between multiple tabs/browsers with the same userId
      const sseUrl = `${this.config.baseUrl}/sync/events?userId=${this.config.userId}&clientId=${encodeURIComponent(this.clientId)}`;
      const eventSource = new EventSource(sseUrl);
      
      // Track if we've resolved the promise (to avoid resolving multiple times on reconnection)
      let resolved = false;

      // Handle connection opened
      eventSource.onopen = () => {
        this.eventSource = eventSource;
        // Note: SSE doesn't immediately give us a sessionId, we'll get it from the first message
      };

      // Handle messages
      eventSource.addEventListener('connected', (event: MessageEvent) => {
        try {
          const message: SSEMessage = JSON.parse(event.data);
          if (message.sessionId) {
            const isReconnection = this.sessionId !== null && this.sessionId === message.sessionId;
            this.sessionId = message.sessionId;
            
            // Only call onConnect callback and resolve on initial connection, not reconnection
            if (!isReconnection && !resolved) {
              if (this.onConnectCallback) {
                this.onConnectCallback(message.sessionId);
              }
              resolve(message.sessionId);
              resolved = true;
            } else if (isReconnection) {
              // On reconnection, just update the sessionId silently
              console.log('[PyreClient] SSE reconnected, sessionId:', message.sessionId);
            }
          }
        } catch (error) {
          console.error('Failed to parse SSE connected message:', error);
          if (!resolved) {
            reject(error instanceof Error ? error : new Error(String(error)));
          }
        }
      });

      eventSource.addEventListener('delta', (event: MessageEvent) => {
        try {
          const message: SSEMessage = JSON.parse(event.data);
          if (this.onMessageCallback) {
            this.onMessageCallback(message);
          }
        } catch (error) {
          console.error('Failed to parse SSE delta message:', error);
        }
      });

      // Handle errors
      eventSource.onerror = (error) => {
        const state = eventSource.readyState;
        const stateNames = ['CONNECTING', 'OPEN', 'CLOSED'];
        
        if (state === EventSource.CLOSED) {
          // Connection closed - EventSource will automatically reconnect
          console.warn(`[PyreClient] SSE connection closed (sessionId: ${this.sessionId || 'none'}). EventSource will auto-reconnect.`);
          if (this.onDisconnectCallback) {
            this.onDisconnectCallback();
          }
        } else if (!this.sessionId && state === EventSource.CONNECTING) {
          // Initial connection failure
          if (this.eventSource === eventSource) {
            console.error('[PyreClient] SSE initial connection failed:', error);
            reject(new Error('SSE connection failed'));
          }
        } else if (state === EventSource.OPEN) {
          // Connection is open but error occurred - this is usually temporary
          console.warn('[PyreClient] SSE error while connection is open (may recover):', error);
        }
      };
    } catch (error) {
      reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  disconnect() {
    this.shouldReconnect = false;
    
    if (this.eventSource) {
      // Close the EventSource to prevent automatic reconnection
      this.eventSource.close();
      this.eventSource = null;
    }

    this.sessionId = null;
  }

  getSessionId(): string | null {
    return this.sessionId;
  }

  isConnected(): boolean {
    return this.eventSource !== null && this.eventSource.readyState === EventSource.OPEN;
  }
}
