import type { SchemaMetadata } from '../../client/src/types';

export interface ServerEndpoints {
  catchup: string;
  events: string;
  query: string;
}

export interface ServerConfig {
  baseUrl: string;
  endpoints?: Partial<ServerEndpoints>;
  headers?: Record<string, string>;
}

export interface SyncProgress {
  table?: string;
  tablesSynced: number;
  totalTables?: number;
  complete: boolean;
  error?: string;
}

export type LiveSyncTransport = 'sse' | 'websocket';

export interface ElmPorts {
  indexedDbOut?: {
    subscribe: (callback: (message: unknown) => void) => void;
  };
  sseOut?: {
    subscribe: (callback: (message: unknown) => void) => void;
  };
  webSocketOut?: {
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
  receiveWebSocketMessage?: {
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
  server: {
    baseUrl: string;
    catchupPath: string;
  };
  liveSync: {
    transport: LiveSyncTransport;
  };
}

export interface ElmModule {
  Main: {
    init: (config: { flags: ElmFlags }) => ElmApp;
  };
}
