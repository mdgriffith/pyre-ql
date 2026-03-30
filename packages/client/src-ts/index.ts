import loadElm from '../dist/engine.mjs';
import { IndexedDBStorage, IndexedDbService } from './service/indexeddb';
import { QueryClientService } from './service/query-client';
import { QueryManagerService } from './service/query-manager';
import { SSEManager, type LiveSyncMessage } from './service/sse';
import { WebSocketManager } from './service/websocket';
import type {
  ElmApp,
  LiveSyncTransport,
  SchemaMetadata,
  ServerConfig,
  ServerEndpoints,
  SyncState,
  SyncStatus,
  SyncProgress,
  TableSyncStatus,
} from './types';

export type {
  LiveSyncTransport,
  ServerConfig,
  ServerEndpoints,
  SyncProgress,
  SyncState,
  SyncStatus,
  TableSyncStatus,
} from './types';
export type { MutationResult } from './service/query-manager';


export interface QueryModule<Input = unknown> {
  operation: 'query' | 'insert' | 'update' | 'delete' | 'mutation';
  id?: string;
  source?: unknown;
  queryShape?: unknown;
  toQueryShape?: (input: Input) => unknown;
}

export interface QuerySubscription<Input = unknown> {
  unsubscribe(): void;
  update(input: Input): void;
}

export interface PyreClientConfig {
  schema: SchemaMetadata;
  server: ServerConfig;
  indexedDbName?: string;
  onError?: (error: Error) => void;
}

export class PyreClient {
  private elmApp: ElmApp;
  private storage: IndexedDBStorage;
  private indexedDbService: IndexedDbService;
  private sseManager: SSEManager;
  private webSocketManager: WebSocketManager;
  private sessionId: string | null = null;
  private syncStateCallbacks: Set<(state: SyncState) => void> = new Set();
  private syncProgressCallbacks: Set<(progress: SyncProgress) => void> = new Set();
  private sessionCallbacks: Set<(sessionId: string) => void> = new Set();
  private lastSyncState: SyncState;
  private lastSyncProgress: SyncProgress | null = null;
  private pendingLiveState: SyncState | null = null;
  private pendingLiveQueries: Set<string> = new Set();
  private queryManager: QueryManagerService;
  private queryClient: QueryClientService;
  private server: ServerConfig;
  private endpoints: ServerEndpoints;
  private queryCounter = 0;

  constructor(config: PyreClientConfig) {
    const dbName = config.indexedDbName ?? 'pyre-client';
    const liveSyncTransport = config.server.liveSyncTransport ?? 'sse';
    this.server = config.server;
    this.endpoints = {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
      ...config.server.endpoints,
    };
    this.lastSyncState = createInitialSyncState(config.schema);
    this.lastSyncProgress = toSyncProgress(this.lastSyncState);

    const elmScope = Object.create(globalThis) as typeof globalThis & { Elm?: unknown };
    elmScope.Elm = undefined;
    let Elm: any;
    try {
      Elm = loadElm(elmScope);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`[PyreClient ctor] loadElm failed: ${message}`);
    }

    try {
      this.elmApp = Elm.Main.init({
        flags: {
          schema: config.schema,
          server: {
            baseUrl: config.server.baseUrl,
            catchupPath: this.endpoints.catchup,
          },
          liveSync: {
            transport: liveSyncTransport,
          },
        },
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`[PyreClient ctor] Elm.Main.init failed: ${message}`);
    }

    this.storage = new IndexedDBStorage(dbName);
    this.indexedDbService = new IndexedDbService(this.storage);
    this.sseManager = new SSEManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
    });
    this.webSocketManager = new WebSocketManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
    });
    this.queryManager = new QueryManagerService();
    this.queryClient = new QueryClientService((payload) => {
      if (config.onError) {
        config.onError(new Error(payload.message));
      } else {
        console.error('[PyreClient] QueryClient error', payload);
      }
    });
    this.queryClient.setOnQueryResult(this.handleQueryResult);
    this.queryClient.setOnQueryUnregister(this.handleQueryUnregister);

    this.indexedDbService.attachPorts(this.elmApp);
    this.sseManager.setOnMessage(this.handleLiveSyncMessage);
    this.webSocketManager.setOnMessage(this.handleLiveSyncMessage);
    this.sseManager.attachPorts(this.elmApp);
    this.webSocketManager.attachPorts(this.elmApp);
    this.queryManager.attachPorts(this.elmApp);
    this.queryClient.attachPorts(this.elmApp);

    if (this.elmApp.ports.errorOut) {
      this.elmApp.ports.errorOut.subscribe((message) => {
        console.log('[PyreClient] port errorOut <-', message);
        const nextState = { ...this.lastSyncState, error: message };
        this.updateSyncState(nextState);
        const error = new Error(message);
        if (config.onError) {
          config.onError(error);
        } else {
          console.error('[PyreClient]', message);
        }
      });
    }

    if (this.elmApp.ports.syncStateOut) {
      this.elmApp.ports.syncStateOut.subscribe((message) => {
        const parsed = parseSyncStateMessage(message);
        if (!parsed) {
          return;
        }

        this.handleRawSyncState(parsed);
      });
    }
  }

  async init(): Promise<void> {
    await this.storage.init();
  }

  onSyncProgress(callback: (progress: SyncProgress) => void): () => void {
    this.syncProgressCallbacks.add(callback);
    if (this.lastSyncProgress) {
      callback(this.lastSyncProgress);
    }
    return () => {
      this.syncProgressCallbacks.delete(callback);
    };
  }

  onSyncState(callback: (state: SyncState) => void): () => void {
    this.syncStateCallbacks.add(callback);
    callback(this.lastSyncState);
    return () => {
      this.syncStateCallbacks.delete(callback);
    };
  }

  onSession(callback: (sessionId: string) => void): () => void {
    this.sessionCallbacks.add(callback);
    if (this.sessionId) {
      callback(this.sessionId);
    }
    return () => {
      this.sessionCallbacks.delete(callback);
    };
  }

  getSessionId(): string | null {
    return this.sessionId;
  }

  run<Input = unknown>(
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): QuerySubscription<Input> | void {
    if (queryModule.operation === 'query') {
      return this.runQuery(queryModule, input, callback);
    }

    this.runMutation(queryModule, input, callback);
    return;
  }

  disconnect(): void {
    this.sseManager.disconnect();
    this.webSocketManager.disconnect();
    this.sessionId = null;
    this.pendingLiveState = null;
    this.pendingLiveQueries.clear();
    const resetState = {
      ...this.lastSyncState,
      status: 'not_started' as const,
      tables: Object.fromEntries(
        Object.keys(this.lastSyncState.tables).map((table) => [table, 'waiting' as const])
      ),
    };
    this.updateSyncState(resetState);
  }

  private handleLiveSyncMessage = (message: LiveSyncMessage): void => {
    if (message.type === 'connected' && message.sessionId) {
      this.sessionId = message.sessionId;
      this.sessionCallbacks.forEach((callback) => {
        callback(message.sessionId!);
      });
    }

    if (message.type === 'syncProgress') {
      const nextState = {
        ...this.lastSyncState,
        status: 'catching_up' as const,
      };
      this.handleRawSyncState(nextState);
    }

    if (message.type === 'syncComplete') {
      const liveTables: Record<string, TableSyncStatus> = {};
      Object.keys(this.lastSyncState.tables).forEach((tableName) => {
        liveTables[tableName] = 'live';
      });
      const nextState = {
        ...this.lastSyncState,
        status: 'live' as const,
        tables: liveTables,
      };
      this.handleRawSyncState(nextState);
    }
  };

  private handleRawSyncState(state: SyncState): void {
    if (state.status !== 'live') {
      this.pendingLiveState = null;
      this.pendingLiveQueries.clear();
      this.updateSyncState(state);
      return;
    }

    const registeredQueryIds = this.queryClient.getRegisteredQueryIds();
    if (registeredQueryIds.length === 0) {
      this.pendingLiveState = null;
      this.pendingLiveQueries.clear();
      this.updateSyncState(state);
      return;
    }

    this.pendingLiveState = state;
    this.pendingLiveQueries = new Set(registeredQueryIds);
    registeredQueryIds.forEach((queryId) => {
      this.queryClient.refreshQuery(queryId);
    });
  }

  private handleQueryResult = (queryId: string): void => {
    if (!this.pendingLiveState) {
      return;
    }

    this.pendingLiveQueries.delete(queryId);
    if (this.pendingLiveQueries.size === 0) {
      const nextState = this.pendingLiveState;
      this.pendingLiveState = null;
      this.updateSyncState(nextState);
    }
  };

  private handleQueryUnregister = (queryId: string): void => {
    if (!this.pendingLiveState) {
      return;
    }

    this.pendingLiveQueries.delete(queryId);
    if (this.pendingLiveQueries.size === 0) {
      const nextState = this.pendingLiveState;
      this.pendingLiveState = null;
      this.updateSyncState(nextState);
    }
  };

  private updateSyncState(state: SyncState): void {
    this.lastSyncState = state;
    this.syncStateCallbacks.forEach((callback) => {
      callback(state);
    });

    const progress = toSyncProgress(state);
    this.lastSyncProgress = progress;
    this.syncProgressCallbacks.forEach((callback) => {
      callback(progress);
    });
  }

  async deleteDatabase(): Promise<void> {
    await this.storage.deleteDatabase();
  }

  private runQuery<Input>(
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): QuerySubscription<Input> {
    const normalizedInput = (input ?? {}) as Input;
    const queryShape = queryModule.toQueryShape
      ? queryModule.toQueryShape(normalizedInput)
      : queryModule.queryShape;

    if (!queryShape) {
      throw new Error('Query module is missing queryShape');
    }

    const queryId = `query_${this.queryCounter}_${Date.now()}`;
    this.queryCounter += 1;

    this.queryClient.registerQuery(
      {
        queryId,
        querySource: queryShape,
        input: normalizedInput,
      },
      callback
    );

    return {
      unsubscribe: () => {
        this.queryClient.unregisterQuery(queryId);
      },
      update: (updatedInput: Input) => {
        this.queryClient.updateQueryInput(queryId, updatedInput);
      },
    };
  }

  private runMutation<Input>(
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): void {
    const id = queryModule.id;
    if (!id) {
      throw new Error('Mutation module is missing id');
    }

    const payload = input ?? {};
    const baseUrl = `${this.server.baseUrl}${this.endpoints.query}`;
    this.queryManager.sendMutation(
      id,
      baseUrl,
      payload,
      callback,
      this.server.headers
    );
  }
}

function createInitialSyncState(schema: SchemaMetadata): SyncState {
  const tables: Record<string, TableSyncStatus> = {};
  Object.keys(schema.tables).forEach((tableName) => {
    tables[tableName] = 'waiting';
  });

  return {
    status: 'not_started',
    tables,
  };
}

function parseSyncStateMessage(message: unknown): SyncState | null {
  if (!message || typeof message !== 'object') {
    return null;
  }

  const candidate = message as {
    status?: unknown;
    tables?: unknown;
  };

  if (
    candidate.status !== 'not_started'
    && candidate.status !== 'catching_up'
    && candidate.status !== 'live'
  ) {
    return null;
  }

  if (!candidate.tables || typeof candidate.tables !== 'object') {
    return null;
  }

  const tableStatuses: Record<string, TableSyncStatus> = {};
  Object.entries(candidate.tables as Record<string, unknown>).forEach(([tableName, tableStatus]) => {
    if (tableStatus === 'waiting' || tableStatus === 'catching_up' || tableStatus === 'live') {
      tableStatuses[tableName] = tableStatus;
    }
  });

  return {
    status: candidate.status,
    tables: tableStatuses,
  };
}

function toSyncProgress(state: SyncState): SyncProgress {
  const tableStatuses = Object.values(state.tables);
  const totalTables = tableStatuses.length;
  const tablesSynced = tableStatuses.filter((status) => status === 'live').length;
  const activeTable = Object.entries(state.tables).find(([, status]) => status === 'catching_up')?.[0];

  return {
    table: activeTable,
    tablesSynced,
    totalTables,
    complete: state.status === 'live',
    error: state.error,
  };
}
