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

/**
 * Schema metadata for relationship information
 */
export interface RelationshipInfo {
  type: 'many-to-one' | 'one-to-many' | null;
  relatedTable: string | null;
  foreignKeyField: string | null;
}

export interface TableMetadata {
  name: string;
  relationships: Record<string, RelationshipInfo>;
}

export interface SchemaMetadata {
  tables: Record<string, TableMetadata>;
  queryFieldToTable: Record<string, string>;
}

/**
 * Input configuration for PyreClient (all optional except required fields)
 */
export interface ClientConfigInput {
  /** Base URL for the server (e.g., "http://localhost:3000") */
  baseUrl: string;
  /** User ID for SSE connection */
  userId: number;
  /** Schema metadata for relationship information */
  schemaMetadata: SchemaMetadata;
  /** Database name for IndexedDB (default: "pyre-client") */
  dbName?: string;
  /** Page size for catchup requests (default: 1000) */
  pageSize?: number;
  /** Headers to include in mutation requests */
  headers?: Record<string, string>;
  /** Error handler callback */
  onError?: (error: Error) => void;
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
  /** SSE reconnection configuration (SSE handles reconnection automatically, but this config is kept for compatibility) */
  reconnect?: {
    /** Initial delay in ms before first reconnect attempt (default: 1000) */
    initialDelay?: number;
    /** Maximum delay in ms between reconnect attempts (default: 30000) */
    maxDelay?: number;
    /** Multiplier for exponential backoff (default: 2) */
    backoffMultiplier?: number;
  };
}

/**
 * Internal configuration with all defaults applied (nothing optional unless truly optional)
 */
export interface ClientConfig {
  /** Base URL for the server */
  baseUrl: string;
  /** User ID for SSE connection */
  userId: number;
  /** Schema metadata for relationship information */
  schemaMetadata: SchemaMetadata;
  /** Database name for IndexedDB */
  dbName: string;
  /** Page size for catchup requests */
  pageSize: number;
  /** Headers to include in mutation requests */
  headers: Record<string, string>;
  /** Error handler callback */
  onError?: (error: Error) => void;
  /** Retry configuration */
  retry: {
    /** Maximum number of retries for catchup */
    maxRetries: number;
    /** Initial delay in ms before first retry */
    initialDelay: number;
    /** Maximum delay in ms between retries */
    maxDelay: number;
    /** Multiplier for exponential backoff */
    backoffMultiplier: number;
  };
  /** SSE reconnection configuration (SSE handles reconnection automatically, but this config is kept for compatibility) */
  reconnect: {
    /** Initial delay in ms before first reconnect attempt */
    initialDelay: number;
    /** Maximum delay in ms between reconnect attempts */
    maxDelay: number;
    /** Multiplier for exponential backoff */
    backoffMultiplier: number;
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
