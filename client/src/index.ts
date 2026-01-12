/**
 * Pyre Client - Browser-side data synchronization and querying
 */

import type { ClientConfigInput, ClientConfig, QueryShape, SyncProgressCallback, SyncStatus, Unsubscribe } from './types';
import { Storage } from './storage';
import { SyncManager } from './sync';
import { SSEManager } from './sse';
import type { SSEMessage } from './sse';
import { QueryManager } from './query';
import { SchemaManager } from './schema';

const DEFAULT_CONFIG: ClientConfig = {
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

function normalizeConfig(input: ClientConfigInput): ClientConfig {
  return {
    baseUrl: input.baseUrl,
    userId: input.userId,
    dbName: input.dbName ?? DEFAULT_CONFIG.dbName,
    pageSize: input.pageSize ?? DEFAULT_CONFIG.pageSize,
    retry: {
      maxRetries: input.retry?.maxRetries ?? DEFAULT_CONFIG.retry.maxRetries,
      initialDelay: input.retry?.initialDelay ?? DEFAULT_CONFIG.retry.initialDelay,
      maxDelay: input.retry?.maxDelay ?? DEFAULT_CONFIG.retry.maxDelay,
      backoffMultiplier: input.retry?.backoffMultiplier ?? DEFAULT_CONFIG.retry.backoffMultiplier,
    },
    reconnect: {
      initialDelay: input.reconnect?.initialDelay ?? DEFAULT_CONFIG.reconnect.initialDelay,
      maxDelay: input.reconnect?.maxDelay ?? DEFAULT_CONFIG.reconnect.maxDelay,
      backoffMultiplier: input.reconnect?.backoffMultiplier ?? DEFAULT_CONFIG.reconnect.backoffMultiplier,
    },
  };
}

export class PyreClient {
  private config: ClientConfig;
  private storage: Storage;
  private syncManager: SyncManager;
  private sseManager: SSEManager;
  private queryManager: QueryManager;
  private schemaManager: SchemaManager;
  private syncProgressCallbacks: Set<SyncProgressCallback> = new Set();
  private initialized = false;
  private quotaCheckInterval: number | null = null;
  private lastSyncedTables: Set<string> = new Set();

  constructor(config: ClientConfigInput) {
    // Convert input config to normalized config with all defaults applied
    this.config = normalizeConfig(config);

    this.storage = new Storage(this.config.dbName);
    this.schemaManager = new SchemaManager();
    this.syncManager = new SyncManager(this.storage, this.config);
    this.sseManager = new SSEManager(this.config);
    this.queryManager = new QueryManager(this.storage, this.schemaManager);

    // Set up SSE message handlers
    this.setupSSEHandlers();

    // Start quota monitoring
    this.startQuotaMonitoring();
  }

  /**
   * Initialize the client - fetches schema, connects SSE and performs initial sync
   */
  async init(onProgress?: SyncProgressCallback): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      // Initialize storage
      await this.storage.init();

      // Fetch and store schema
      try {
        const { schema, introspection } = await this.schemaManager.fetchSchema(this.config.baseUrl);
        // Store the introspection JSON as a string
        await this.storage.saveSchema(JSON.stringify(introspection));
      } catch (error) {
        console.error('[PyreClient] Failed to fetch schema, trying cached version:', error);
        // Try to load cached schema
        const cachedSchemaJson = await this.storage.getSchema();
        if (cachedSchemaJson) {
          try {
            // Try to parse as introspection JSON
            const introspection = JSON.parse(cachedSchemaJson);
            if (introspection.tables) {
              this.schemaManager.setIntrospectionJson(introspection);
            } else {
              throw new Error('Invalid cached schema format');
            }
          } catch (parseError) {
            console.error('[PyreClient] Failed to parse cached schema:', parseError);
            throw new Error('No valid schema available and failed to fetch');
          }
        } else {
          throw new Error('No schema available and failed to fetch');
        }
      }

      // Set up progress callback
      if (onProgress) {
        this.onSyncProgress(onProgress);
      }

      // Connect SSE
      const sessionId = await this.sseManager.connect();
      this.syncManager.setSessionId(sessionId);

      // Perform initial sync (resumes from cursor if available)
      await this.syncManager.sync((progress) => {
        this.syncProgressCallbacks.forEach(cb => {
          try {
            cb(progress);
          } catch (error) {
            console.error('[PyreClient] Sync progress callback failed:', error);
          }
        });
      });

      // Get synced tables from sync manager
      const syncedTables = this.syncManager.getSyncedTables();

      // Store synced tables for queries created after sync
      this.lastSyncedTables = syncedTables;

      // After sync completes, notify all queries so they pick up new data
      // Use a small delay to ensure queries have been set up
      if (syncedTables.size > 0) {
        // Use requestAnimationFrame to ensure DOM is ready, then notify
        requestAnimationFrame(() => {
          setTimeout(() => {
            this.queryManager.notifyQueries(Array.from(syncedTables));
          }, 100);
        });
      }

      this.initialized = true;
    } catch (error) {
      console.error('[PyreClient] Initialization failed:', error);
      throw error;
    }
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
    const unsubscribe = this.queryManager.query(shape, callback);
    
    // If sync has already completed, notify this query immediately
    // so it picks up data that was synced before the query was created
    if (this.initialized && this.lastSyncedTables.size > 0) {
      // Extract table dependencies from query shape
      const queryTables = new Set<string>();
      for (const tableName of Object.keys(shape)) {
        queryTables.add(tableName);
      }
      
      // Check if any of the query's tables were synced
      const hasSyncedTables = Array.from(queryTables).some(table => this.lastSyncedTables.has(table));
      if (hasSyncedTables) {
        // Notify immediately so the query picks up synced data
        setTimeout(() => {
          this.queryManager.notifyQueries(Array.from(queryTables));
        }, 0);
      }
    }
    
    return unsubscribe;
  }

  /**
   * Disconnect and cleanup
   */
  disconnect(): void {
    this.sseManager.disconnect();
    this.syncProgressCallbacks.clear();
    if (this.quotaCheckInterval !== null) {
      clearInterval(this.quotaCheckInterval);
      this.quotaCheckInterval = null;
    }
    this.initialized = false;
  }

  /**
   * Delete the IndexedDB database and reset state
   */
  async deleteDatabase(): Promise<void> {
    this.disconnect();
    await this.storage.deleteDatabase();
    this.initialized = false;
  }

  private startQuotaMonitoring(): void {
    // Check quota every 30 seconds
    this.quotaCheckInterval = window.setInterval(async () => {
      try {
        const { percentage } = await this.storage.checkStorageQuota();
        if (percentage > 75) {
          console.warn(
            `[PyreClient] Storage usage is ${percentage.toFixed(1)}% full. ` +
            `Consider cleaning up old data or increasing storage quota.`
          );
        }
      } catch (error) {
        console.error('[PyreClient] Failed to check storage quota:', error);
      }
    }, 30000);
  }

  private setupSSEHandlers(): void {
    // Handle SSE messages
    this.sseManager.onMessage((message: SSEMessage) => {
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

          // Notify only queries that depend on affected tables
          this.queryManager.notifyQueries(Array.from(affectedTables));
        }).catch(error => {
          console.error('[PyreClient] Failed to apply delta:', error);
        });
      }
    });

    // Handle connection - resume sync from cursor (not full resync)
    this.sseManager.onConnect((sessionId: string) => {
      this.syncManager.setSessionId(sessionId);
      // Resume sync from cursor after connection (not full resync)
      this.syncManager.sync((progress) => {
        this.syncProgressCallbacks.forEach(cb => {
          try {
            cb(progress);
          } catch (error) {
            console.error('[PyreClient] Sync progress callback failed:', error);
          }
        });
      }).catch(error => {
        console.error('[PyreClient] Failed to sync after connection:', error);
      });
    });
  }
}

// Export types
export type {
  ClientConfigInput,
  QueryShape,
  WhereClause,
  SortClause,
  SyncProgressCallback,
  SyncStatus,
  Unsubscribe,
} from './types';
