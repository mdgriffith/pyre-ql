import type { SchemaMetadata } from '../../client/src/types';

export interface SSEConfig {
  baseUrl: string;
  userId: number;
}

export interface SyncProgress {
  table?: string;
  tablesSynced: number;
  totalTables?: number;
  complete: boolean;
  error?: string;
}

export interface ElmPorts {
  indexedDbOut?: {
    subscribe: (callback: (message: unknown) => void) => void;
  };
  sseOut?: {
    subscribe: (callback: (message: unknown) => void) => void;
  };
  queryManagerOut?: {
    subscribe: (callback: (message: unknown) => void) => void;
  };
  errorOut?: {
    subscribe: (callback: (message: string) => void) => void;
  };
  receiveIndexedDbMessage?: {
    send: (message: unknown) => void;
  };
  receiveSSEMessage?: {
    send: (message: unknown) => void;
  };
  receiveQueryManagerMessage?: {
    send: (message: unknown) => void;
  };
}

export interface ElmApp {
  ports: ElmPorts;
}

export interface ElmFlags {
  schema: SchemaMetadata;
  sseConfig: SSEConfig;
}

export interface ElmModule {
  Main: {
    init: (config: { flags: ElmFlags }) => ElmApp;
  };
}
