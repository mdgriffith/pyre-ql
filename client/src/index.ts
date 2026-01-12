/**
 * Pyre Client - Browser-side data synchronization and querying
 */

import type { ClientConfig, QueryShape, SyncProgressCallback, SyncStatus, Unsubscribe } from './types';
import { Storage } from './storage';
import { SyncManager } from './sync';
import { WebSocketManager } from './websocket';
import type { WebSocketMessage } from './websocket';
import { QueryManager } from './query';

const DEFAULT_CONFIG: Required<ClientConfig> = {
  baseUrl: '',
  userId: 0,
  dbName: 'pyre-client',
  pageSize: 1000,
  retry: {
    maxRetries: 5,
    initialDelay: 1000,
    maxDelay: 30000,
    backoffMultiplier: 2,
  },
  reconnect: {
    initialDelay: 1000,
    maxDelay: 30000,
    backoffMultiplier: 2,
  },
};

export class PyreClient {
  private config: Required<ClientConfig>;
  private storage: Storage;
  private syncManager: SyncManager;
  private wsManager: WebSocketManager;
  private queryManager: QueryManager;
  private syncProgressCallbacks: Set<SyncProgressCallback> = new Set();
  private initialized = false;

  constructor(config: ClientConfig) {
    // Merge with defaults
    this.config = {
      ...DEFAULT_CONFIG,
      ...config,
      retry: {
        ...DEFAULT_CONFIG.retry,
        ...config.retry,
      },
      reconnect: {
        ...DEFAULT_CONFIG.reconnect,
        ...config.reconnect,
      },
    };

    this.storage = new Storage(this.config.dbName);
    this.syncManager = new SyncManager(this.storage, this.config);
    this.wsManager = new WebSocketManager(this.config);
    this.queryManager = new QueryManager(this.storage);

    // Set up WebSocket message handlers
    this.setupWebSocketHandlers();
  }

  /**
   * Initialize the client - connects WebSocket and performs initial sync
   */
  async init(onProgress?: SyncProgressCallback): Promise<void> {
    if (this.initialized) {
      return;
    }

    // Initialize storage
    await this.storage.init();

    // Set up progress callback
    if (onProgress) {
      this.onSyncProgress(onProgress);
    }

    // Connect WebSocket
    const sessionId = await this.wsManager.connect();
    this.syncManager.setSessionId(sessionId);

    // Perform initial sync
    await this.syncManager.sync((progress) => {
      this.syncProgressCallbacks.forEach(cb => cb(progress));
    });

    this.initialized = true;
  }

  /**
   * Register a callback for sync progress updates
   */
  onSyncProgress(callback: SyncProgressCallback): Unsubscribe {
    this.syncProgressCallbacks.add(callback);
    return () => {
      this.syncProgressCallbacks.delete(callback);
    };
  }

  /**
   * Get current sync status
   */
  getSyncStatus(): SyncStatus {
    return this.syncManager.getStatus();
  }

  /**
   * Execute a query with live updates
   * @param shape - Query shape (GraphQL-like)
   * @param callback - Callback function that receives query results
   * @returns Unsubscribe function
   */
  query(shape: QueryShape, callback: (data: any) => void): Unsubscribe {
    return this.queryManager.query(shape, callback);
  }

  /**
   * Disconnect and cleanup
   */
  disconnect(): void {
    this.wsManager.disconnect();
    this.syncProgressCallbacks.clear();
    this.initialized = false;
  }

  private setupWebSocketHandlers(): void {
    // Handle WebSocket messages
    this.wsManager.onMessage((message: WebSocketMessage) => {
      if (message.type === 'delta') {
        // Apply delta and notify queries
        this.syncManager.applyDelta(message.data).then(() => {
          // Extract affected table names from delta
          const affectedTables = new Set<string>();
          if (message.data?.all_affected_rows) {
            for (const row of message.data.all_affected_rows) {
              if (row.table_name) {
                affectedTables.add(row.table_name);
              }
            }
          }
          
          // Notify all active queries
          this.queryManager.notifyQueries(Array.from(affectedTables));
        }).catch(error => {
          console.error('Failed to apply delta:', error);
        });
      }
    });

    // Handle reconnection - re-sync when reconnected
    this.wsManager.onConnect((sessionId: string) => {
      this.syncManager.setSessionId(sessionId);
      // Re-sync after reconnection
      this.syncManager.sync((progress) => {
        this.syncProgressCallbacks.forEach(cb => cb(progress));
      }).catch(error => {
        console.error('Failed to sync after reconnection:', error);
      });
    });
  }
}

// Export types
export type {
  ClientConfig,
  QueryShape,
  WhereClause,
  SortClause,
  SyncProgressCallback,
  SyncStatus,
  Unsubscribe,
} from './types';
