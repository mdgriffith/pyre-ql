/**
 * Type definitions for Pyre Client
 */

export interface SyncCursor {
  tables: Record<string, {
    last_seen_updated_at: number | null;
    permission_hash: string;
  }>;
}

export interface SyncPageResult {
  tables: Record<
    string,
    {
      rows: any[];
      permission_hash: string;
      last_seen_updated_at: number | null;
    }
  >;
  has_more: boolean;
}

export interface ClientConfig {
  /** Base URL for the server (e.g., "http://localhost:3000") */
  baseUrl: string;
  /** User ID for WebSocket connection */
  userId: number;
  /** Database name for IndexedDB (default: "pyre-client") */
  dbName?: string;
  /** Page size for catchup requests (default: 1000) */
  pageSize?: number;
  /** Retry configuration */
  retry?: {
    /** Maximum number of retries for catchup (default: 5) */
    maxRetries?: number;
    /** Initial delay in ms before first retry (default: 1000) */
    initialDelay?: number;
    /** Maximum delay in ms between retries (default: 30000) */
    maxDelay?: number;
    /** Multiplier for exponential backoff (default: 2) */
    backoffMultiplier?: number;
  };
  /** WebSocket reconnection configuration */
  reconnect?: {
    /** Initial delay in ms before first reconnect attempt (default: 1000) */
    initialDelay?: number;
    /** Maximum delay in ms between reconnect attempts (default: 30000) */
    maxDelay?: number;
    /** Multiplier for exponential backoff (default: 2) */
    backoffMultiplier?: number;
  };
}

export type FilterValue =
  | string
  | number
  | boolean
  | null
  | { $eq?: FilterValue; $ne?: FilterValue; $gt?: FilterValue; $lt?: FilterValue; $gte?: FilterValue; $lte?: FilterValue; $in?: FilterValue[] };

export interface WhereClause {
  $and?: WhereClause[];
  $or?: WhereClause[];
  [field: string]: FilterValue | WhereClause | WhereClause[] | undefined;
}

export type SortDirection = 'asc' | 'desc' | 'Asc' | 'Desc';

export interface SortClause {
  field: string;
  direction: SortDirection;
}

export interface QueryField {
  '@where'?: WhereClause;
  '@sort'?: SortClause | SortClause[];
  '@limit'?: number;
  [field: string]: boolean | QueryField | WhereClause | SortClause | SortClause[] | number | undefined;
}

export interface QueryShape {
  [tableName: string]: QueryField;
}

export type Unsubscribe = () => void;

export interface QuerySubscription {
  data: any;
  unsubscribe: Unsubscribe;
}

export type SyncProgressCallback = (progress: {
  /** Current table being synced */
  table?: string;
  /** Total tables synced so far */
  tablesSynced: number;
  /** Total tables to sync */
  totalTables?: number;
  /** Whether sync is complete */
  complete: boolean;
  /** Error if sync failed */
  error?: Error;
}) => void;

export interface SyncStatus {
  /** Whether sync is currently in progress */
  syncing: boolean;
  /** Whether initial sync is complete */
  synced: boolean;
  /** Last sync error, if any */
  error?: Error;
}
