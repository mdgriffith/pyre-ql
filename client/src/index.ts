/**
 * Pyre Client - Browser-side data synchronization and querying
 */

import * as Ark from 'arktype';
import type { ClientConfigInput, ClientConfig, QueryShape, SyncProgressCallback, SyncStatus, Unsubscribe, QuerySubscription, Query } from './types';
import { Storage } from './storage';
import { SyncManager } from './sync';
import { SSEManager } from './sse';
import type { SSEMessage } from './sse';
import { QueryManager } from './query';
import { SchemaManager } from './schema';

const DEFAULT_CONFIG: Pick<ClientConfig, 'dbName' | 'pageSize' | 'headers' | 'retry' | 'reconnect'> = {
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
      this.notifyQueriesForTables(syncedTables);

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
  run<T extends Query & {
    toQueryShape?: (input: T['InputValidator']['infer']) => QueryShape;
  }>(
    queryModule: T,
    input: T['InputValidator']['infer'] extends Record<string, never> ? undefined : T['InputValidator']['infer'],
    callback: (result: T['ReturnData']['infer']) => void
  ): QuerySubscription<T['InputValidator']['infer']> | void {
    const queryName = queryModule.hash || 'unknown';

    // Decode input if provided
    if (input !== undefined) {
      const decodedInput = this.decodeInput<T['InputValidator']['infer']>(queryModule.InputValidator, input, queryName);
      if (decodedInput === null) {
        return; // Error already handled
      }
      input = decodedInput;
    }

    if (queryModule.operation === 'query') {
      return this.runQuery(queryModule as T & { toQueryShape: (input: T['InputValidator']['infer']) => QueryShape }, input, callback, queryName);
    } else {
      this.runMutation(queryModule, input, callback, queryName);
      return;
    }
  }

  /**
   * Execute a query against IndexedDB
   */
  private runQuery<T extends Query>(
    queryModule: T & { toQueryShape: (input: T['InputValidator']['infer']) => QueryShape },
    input: T['InputValidator']['infer'] | undefined,
    callback: (result: T['ReturnData']['infer']) => void,
    queryName: string
  ): QuerySubscription<T['InputValidator']['infer']> {

    // Normalize input (use empty object if undefined)
    const normalizedInput = input ?? {} as T['InputValidator']['infer'];
    const initialShape = queryModule.toQueryShape(normalizedInput);

    const subscription = this.queryManager.query(
      queryModule.toQueryShape,
      normalizedInput,
      (data: unknown) => {
        if (!data) {
          console.warn(`[PyreClient] Query "${queryName}" returned no data`);
          const emptyResult = this.buildEmptyResult(initialShape);
          callback(emptyResult as T['ReturnData']['infer']);
          return;
        }

        console.log(`[PyreClient] Query "${queryName}" raw data:`, JSON.stringify(data, null, 2));

        const decodedData = this.decodeReturnData<T['ReturnData']['infer']>(queryModule.ReturnData, data, queryName);
        if (decodedData === null) {
          const emptyResult = this.buildEmptyResult(initialShape);
          callback(emptyResult as T['ReturnData']['infer']);
          return;
        }

        callback(decodedData);
      },
      queryModule.InputValidator,
      (error: Error) => this.handleError(error)
    );

    // If sync has already completed, notify this query immediately
    if (this.initialized && this.lastSyncedTables.size > 0) {
      const queryTableNames = this.extractTableNamesFromShape(initialShape);
      const hasSyncedTables = Array.from(queryTableNames).some(table => this.lastSyncedTables.has(table));
      if (hasSyncedTables) {
        this.notifyQueriesForTables(queryTableNames);
      }
    }

    return subscription;
  }

  /**
   * Execute a mutation against the server
   */
  private runMutation<T extends Query>(
    queryModule: T,
    input: T['InputValidator']['infer'] | undefined,
    callback: (result: T['ReturnData']['infer']) => void,
    queryName: string
  ): void {
    const hash = queryModule.hash;
    if (!hash) {
      this.handleError(new Error('Mutation module missing hash'));
      return;
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
          this.handleError(new Error(`HTTP error! status: ${response.status}`));
          return;
        }
        const data = await response.json();

        const decodedData = this.decodeReturnData<T['ReturnData']['infer']>(queryModule.ReturnData, data, queryName);
        if (decodedData !== null) {
          callback(decodedData);
        }
      })
      .catch((error) => {
        this.handleError(error as Error);
      });
  }

  /**
   * Decode input using ArkType decoder
   * Returns decoded data if successful, null if decoding failed
   */
  private decodeInput<T>(decoder: { (input: any): T | Ark.ArkErrors }, input: unknown, queryName: string): T | null {
    const decoded = decoder(input);

    if (decoded instanceof Ark.type.errors) {
      let errorMessage = 'Invalid input';
      if (decoded && typeof decoded === 'object') {
        const errorStr = JSON.stringify(decoded, null, 2);
        errorMessage = `Validation failed: ${errorStr}`;
      } else if (typeof decoded === 'string') {
        errorMessage = `Validation failed: ${decoded}`;
      }
      this.handleError(new Error(`Query "${queryName}" input validation failed: ${errorMessage}`));
      return null;
    }

    return decoded as T;
  }

  /**
   * Decode return data using ArkType decoder
   * Returns decoded data if successful, null if decoding failed
   */
  private decodeReturnData<T>(decoder: { (input: any): T | Ark.ArkErrors }, data: unknown, queryName: string): T | null {
    const decoded = decoder(data);
    console.log(`[PyreClient] Query "${queryName}" validation result:`, decoded);

    if (decoded instanceof Ark.type.errors) {
      let errorMessage = 'Invalid return data';
      if (decoded && typeof decoded === 'object') {
        const errorStr = JSON.stringify(decoded, null, 2);
        errorMessage = `Validation failed: ${errorStr}`;
      } else if (typeof decoded === 'string') {
        errorMessage = `Validation failed: ${decoded}`;
      }
      console.error(`[PyreClient] Query "${queryName}" return data validation failed:`, errorMessage);
      console.error('[PyreClient] Data that failed validation:', JSON.stringify(data, null, 2));
      this.handleError(new Error(`Query "${queryName}" return data validation failed: ${errorMessage}`));
      return null;
    }

    return decoded as T;
  }

  /**
   * Build an empty result structure matching the query shape
   */
  private buildEmptyResult(shape: QueryShape): Record<string, never[]> {
    const result: Record<string, never[]> = {};
    for (const queryFieldName of Object.keys(shape)) {
      result[queryFieldName] = [];
    }
    return result;
  }

  /**
   * Extract table names from a query shape
   */
  private extractTableNamesFromShape(shape: QueryShape): Set<string> {
    const tableNames = new Set<string>();
    for (const queryFieldName of Object.keys(shape)) {
      const tableName = this.schemaManager.getTableNameFromQueryField(queryFieldName);
      if (tableName) {
        tableNames.add(tableName);
      }
    }
    return tableNames;
  }

  /**
   * Notify queries for affected tables
   */
  private notifyQueriesForTables(tableNames: Set<string>): void {
    if (tableNames.size > 0) {
      // Use setTimeout to defer notification to next tick
      setTimeout(() => {
        this.queryManager.notifyQueries(Array.from(tableNames));
      }, 0);
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
          if (Array.isArray(message.data)) {
            for (const row of message.data) {
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
  Query,
  WhereClause,
  SortClause,
  SyncProgressCallback,
  SyncStatus,
  Unsubscribe,
  QuerySubscription,
  SchemaMetadata,
  TableMetadata,
  LinkInfo,
} from './types';
