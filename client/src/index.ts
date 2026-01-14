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

const DEFAULT_CONFIG: Partial<ClientConfig> = {
  baseUrl: '',
  userId: 0,
  dbName: 'pyre-client',
  pageSize: 1000,
  headers: {},
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
    schemaMetadata: input.schemaMetadata,
    dbName: input.dbName ?? DEFAULT_CONFIG.dbName,
    pageSize: input.pageSize ?? DEFAULT_CONFIG.pageSize,
    headers: input.headers ?? {},
    onError: input.onError,
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
    this.schemaManager.setSchemaMetadata(this.config.schemaMetadata);
    this.syncManager = new SyncManager(this.storage, this.config);
    this.sseManager = new SSEManager(this.config);
    this.queryManager = new QueryManager(this.storage, this.schemaManager);

    // Set up SSE message handlers
    this.setupSSEHandlers();

    // Start quota monitoring
    this.startQuotaMonitoring();
  }

  /**
   * Initialize the client - connects SSE and performs initial sync
   */
  async init(onProgress?: SyncProgressCallback): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      // Initialize storage
      await this.storage.init();

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
      this.handleError(error as Error);
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
   * Execute a query or mutation with type safety
   * @param queryModule - Generated query/mutation module with hash, operation, InputValidator, ReturnData, and queryShape (for queries)
   * @param input - Input parameters (optional if query has no parameters)
   * @param callback - Callback function that receives typed results
   * @returns Unsubscribe function (for queries) or void (for mutations)
   */
  run<T extends {
    hash?: string;
    operation: 'query' | 'insert' | 'update' | 'delete';
    InputValidator: { infer: any; (input: any): any };
    ReturnData: { infer: any; (input: any): any };
    queryShape?: QueryShape;
  }>(
    queryModule: T,
    input: T['InputValidator']['infer'] extends Record<string, never> ? undefined : T['InputValidator']['infer'],
    callback: (result: T['ReturnData']['infer']) => void
  ): Unsubscribe | void {
    try {
      // Validate input if provided
      if (input !== undefined) {
        const validation = queryModule.InputValidator(input as any);
        // ArkType returns an error object if validation fails, or the validated data if successful
        // Check if validation failed (has 'summary' or 'problems' property)
        if (validation && typeof validation === 'object' && ('summary' in validation || 'problems' in validation)) {
          const queryName = (queryModule as any).hash || 'unknown';
          const errorDetails = (validation as any).summary || (validation as any).problems || validation;
          const error = new Error(`Query "${queryName}" input validation failed: ${JSON.stringify(errorDetails)}`);
          this.handleError(error);
          throw error;
        }
      }

      if (queryModule.operation === 'query') {
        // Execute query against IndexedDB
        if (!queryModule.queryShape) {
          throw new Error('Query module missing queryShape');
        }

        const unsubscribe = this.queryManager.query(queryModule.queryShape, (data: any) => {
          try {
            const queryName = (queryModule as any).hash || 'unknown';

            if (!data) {
              console.warn(`[PyreClient] Query "${queryName}" returned no data`);
              // Return empty structure matching ReturnData shape
              callback({ user: [], post: [] } as T['ReturnData']['infer']);
              return;
            }

            console.log(`[PyreClient] Query "${queryName}" raw data:`, JSON.stringify(data, null, 2));

            // Validate and decode return data
            // ArkType returns the validated data directly if valid, or an error object if invalid
            let validation: any;
            try {
              validation = queryModule.ReturnData(data);
              console.log(`[PyreClient] Query "${queryName}" validation result:`, validation);
            } catch (validationError) {
              const error = new Error(`Query "${queryName}" return data validation threw error: ${validationError}`);
              this.handleError(error);
              // Return empty structure on validation error
              callback({ user: [], post: [] } as T['ReturnData']['infer']);
              return;
            }

            // Check if validation failed
            // ArkType errors are instances of type.errors or have a 'summary' property
            if (validation && typeof validation === 'object' && ('summary' in validation || 'problems' in validation)) {
              const errorDetails = (validation as any).summary || (validation as any).problems || validation;
              console.error(`[PyreClient] Query "${queryName}" return data validation failed:`, errorDetails);
              console.error('[PyreClient] Data that failed validation:', JSON.stringify(data, null, 2));
              const error = new Error(`Query "${queryName}" return data validation failed: ${JSON.stringify(errorDetails)}`);
              this.handleError(error);
              // Return empty structure on validation failure
              callback({ user: [], post: [] } as T['ReturnData']['infer']);
              return;
            }

            // Validation succeeded - use the validated data directly
            // The validation result IS the validated data
            if (validation === null || validation === undefined) {
              console.warn(`[PyreClient] Query "${queryName}" validation returned null/undefined, using original data`);
              // Fallback to original data if validation returned undefined/null
              // Ensure it has the right structure
              const fallbackData = {
                user: Array.isArray(data?.user) ? data.user : [],
                post: Array.isArray(data?.post) ? data.post : []
              };
              callback(fallbackData as T['ReturnData']['infer']);
              return;
            }

            // Ensure validation result has the expected structure
            const finalData = {
              user: Array.isArray(validation?.user) ? validation.user : [],
              post: Array.isArray(validation?.post) ? validation.post : []
            };

            callback(finalData as T['ReturnData']['infer']);
          } catch (error) {
            console.error('[PyreClient] Unexpected error in query callback:', error);
            this.handleError(error as Error);
            // Return empty structure on unexpected error
            callback({ user: [], post: [] } as T['ReturnData']['infer']);
          }
        });

        // If sync has already completed, notify this query immediately
        if (this.initialized && this.lastSyncedTables.size > 0) {
          // Map query field names to table names for notification
          const queryTableNames = new Set<string>();
          for (const queryFieldName of Object.keys(queryModule.queryShape)) {
            const tableName = this.schemaManager.getTableNameFromQueryField(queryFieldName);
            if (tableName) {
              queryTableNames.add(tableName);
            }
          }

          const hasSyncedTables = Array.from(queryTableNames).some(table => this.lastSyncedTables.has(table));
          if (hasSyncedTables) {
            setTimeout(() => {
              this.queryManager.notifyQueries(Array.from(queryTableNames));
            }, 0);
          }
        }

        return unsubscribe;
      } else {
        // Execute mutation against server
        const hash = queryModule.hash;
        if (!hash) {
          throw new Error('Mutation module missing hash');
        }

        const url = `${this.config.baseUrl}/${hash}`;
        const body = input || {};

        fetch(url, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...this.config.headers,
          },
          body: JSON.stringify(body),
        })
          .then(async (response) => {
            if (!response.ok) {
              throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();

            // Validate return data
            const validation = queryModule.ReturnData(data);
            if (validation.problems) {
              const error = new Error(`Return data validation failed: ${JSON.stringify(validation.problems)}`);
              this.handleError(error);
              return;
            }
            callback(validation.data as T['ReturnData']['infer']);
          })
          .catch((error) => {
            this.handleError(error);
          });

        return;
      }
    } catch (error) {
      this.handleError(error as Error);
      throw error;
    }
  }

  private handleError(error: Error): void {
    console.error('[PyreClient]', error);
    if (this.config.onError) {
      try {
        this.config.onError(error);
      } catch (callbackError) {
        console.error('[PyreClient] Error handler threw an error:', callbackError);
      }
    }
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
  ClientConfig,
  QueryShape,
  QueryField,
  WhereClause,
  SortClause,
  SyncProgressCallback,
  SyncStatus,
  Unsubscribe,
  SchemaMetadata,
  TableMetadata,
  RelationshipInfo,
} from './types';
