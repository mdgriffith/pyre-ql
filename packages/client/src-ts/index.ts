import loadElm from '../dist/engine.mjs';
import { IndexedDBStorage, IndexedDbService } from './service/indexeddb';
import { QueryClientService } from './service/query-client';
import { QueryManagerService, type MutationResult } from './service/query-manager';
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
  ServerHeaders,
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

export interface RegisteredQuery<Input = unknown> {
  queryId: string;
  querySource: unknown;
  input: Input;
  queryName?: string;
}

export interface RegisteredQueryResult {
  queryId: string;
  queryName?: string;
  revision: number;
  result: unknown;
}

export interface PyreDevtoolsEvent {
  id: number;
  timestamp: number;
  type: string;
  payload?: unknown;
}

export interface PyreDevtoolsTableSnapshot {
  name: string;
  count: number;
  rows: unknown[];
  sync?: TableSyncStatus;
  cursor?: {
    last_seen_updated_at: number | null;
    permission_hash: string;
  };
}

export interface PyreDevtoolsSnapshot {
  schema: SchemaMetadata;
  indexedDbName: string;
  server: {
    baseUrl: string;
    endpoints: ServerEndpoints;
    liveSyncTransport: LiveSyncTransport;
    hasHeaders: boolean;
    credentials: RequestCredentials;
    withCredentials: boolean;
  };
  connectionId: string | null;
  syncState: SyncState;
  syncProgress: SyncProgress | null;
  debugValues: Record<string, unknown>;
  tables: Record<string, PyreDevtoolsTableSnapshot>;
}

export interface ElmBridgePort {
  subscribe?: (callback: (message: unknown) => void) => void;
  unsubscribe?: (callback: (message: unknown) => void) => void;
  send?: (message: unknown) => void;
}

export interface ElmBridgeApp {
  ports?: Record<string, ElmBridgePort | undefined>;
}

export interface ElmBridgeQueryMessage {
  type: 'register' | 'update-input' | 'unregister';
  queryId: string;
  queryName?: string;
  querySource?: unknown;
  queryInput?: unknown;
}

export interface ElmBridgeMutationMessage {
  type: 'mutate';
  requestId: string;
  mutationId: string;
  mutationName?: string;
  mutationInput?: unknown;
}

export interface ElmBridgeMutationResultMessage {
  type: 'mutation-result';
  requestId: string;
  mutationId: string;
  mutationName: string | null;
  result: MutationResult;
}

export type ElmBridgeIncomingMessage = ElmBridgeQueryMessage | ElmBridgeMutationMessage;

export interface ElmBridgeConfig {
  app: ElmBridgeApp;
  receivePort?: string;
  queryResultPort?: string;
  syncStatePort?: string;
  mutationResultPort?: string;
  onMutation?: (payload: {
    client: PyreClient;
    message: ElmBridgeMutationMessage;
  }) => Promise<void> | void;
  onError?: (error: Error, context: { phase: 'incoming-message' | 'mutation-handler' }) => void;
}

export interface PyreElmConfig extends ElmBridgeConfig {}

export interface PyreClientConfig {
  schema: SchemaMetadata;
  server: ServerConfig;
  resolvedHeaders?: Record<string, string>;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  elm?: PyreElmConfig;
}

export interface PyreClientCreateConfig {
  schema: SchemaMetadata;
  server?: ServerConfig;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  connect?: () => Promise<{
    server: ServerConfig;
    session?: Record<string, unknown>;
  }> | {
    server: ServerConfig;
    session?: Record<string, unknown>;
  };
  elm?: PyreElmConfig;
}

export class PyreClient {
  private elmApp: ElmApp;
  private storage: IndexedDBStorage;
  private indexedDbService: IndexedDbService;
  private sseManager: SSEManager;
  private webSocketManager: WebSocketManager;
  private connectionId: string | null = null;
  private syncStateCallbacks: Set<(state: SyncState) => void> = new Set();
  private syncProgressCallbacks: Set<(progress: SyncProgress) => void> = new Set();
  private connectionCallbacks: Set<(connectionId: string) => void> = new Set();
  private lastSyncState: SyncState;
  private lastSyncProgress: SyncProgress | null = null;
  private pendingLiveState: SyncState | null = null;
  private pendingLiveQueries: Set<string> = new Set();
  private queryManager: QueryManagerService;
  private queryClient: QueryClientService;
  private bridgeCleanup: (() => void) | null = null;
  private debug: boolean;
  private session: Record<string, unknown>;
  private server: ServerConfig;
  private endpoints: ServerEndpoints;
  private queryCounter = 0;
  private mutationCounter = 0;
  private schema: SchemaMetadata;
  private indexedDbName: string;
  private devtoolsDebugValues: Record<string, unknown> = {};
  private devtoolsEventCounter = 0;
  private devtoolsEventCallbacks: Set<(event: PyreDevtoolsEvent) => void> = new Set();

  constructor(config: PyreClientConfig) {
    const dbName = config.indexedDbName ?? 'pyre-client';
    const liveSyncTransport = config.server.liveSyncTransport ?? 'sse';
    this.debug = config.debug ?? false;
    this.server = config.server;
    this.schema = config.schema;
    this.indexedDbName = dbName;
    this.endpoints = {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
      ...config.server.endpoints,
    };
    this.lastSyncState = createInitialSyncState(config.schema);
    this.lastSyncProgress = toSyncProgress(this.lastSyncState);
    this.session = config.session ?? {};

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
      const initialHeaders = config.resolvedHeaders ?? getStaticServerHeaders(config.server);
      this.elmApp = Elm.Main.init({
        flags: {
          schema: config.schema,
          server: {
            baseUrl: config.server.baseUrl,
            catchupPath: this.endpoints.catchup,
            headers: serverHeadersToPairs(initialHeaders),
            credentials: getServerCredentials(config.server),
            withCredentials: shouldIncludeCredentials(config.server),
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
    this.indexedDbService = new IndexedDbService(this.storage, this.logDebug);
    this.sseManager = new SSEManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
      credentials: config.server.credentials,
      withCredentials: config.server.withCredentials,
    }, undefined, this.logDebug);
    this.webSocketManager = new WebSocketManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
    }, undefined, this.logDebug);
    this.queryManager = new QueryManagerService(this.logDebug);
    this.queryClient = new QueryClientService(() => this.session, (payload) => {
      if (config.onError) {
        config.onError(new Error(payload.message));
      } else {
        console.error('[PyreClient] QueryClient error', payload);
      }
    }, this.logDebug);
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
        this.logDebug('[PyreClient] port errorOut <-', message);
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

  private logDebug = (...args: unknown[]): void => {
    this.emitDevtoolsEvent('debug', { args });
    if (this.debug) {
      console.log(...args);
    }
  };

  static async create(config: PyreClientCreateConfig): Promise<PyreClient> {
    const resolvedConfig = await resolveCreateConfig(config);
    const client = new PyreClient(resolvedConfig);
    await client.init();
    if (resolvedConfig.elm) {
      client.bridgeCleanup = client.attachElmBridge(resolvedConfig.elm);
    }
    return client;
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

  onConnection(callback: (connectionId: string) => void): () => void {
    this.connectionCallbacks.add(callback);
    if (this.connectionId) {
      callback(this.connectionId);
    }
    return () => {
      this.connectionCallbacks.delete(callback);
    };
  }

  onDevtoolsEvent(callback: (event: PyreDevtoolsEvent) => void): () => void {
    this.devtoolsEventCallbacks.add(callback);
    return () => {
      this.devtoolsEventCallbacks.delete(callback);
    };
  }

  async getDevtoolsSnapshot(): Promise<PyreDevtoolsSnapshot> {
    const [tables, cursor] = await Promise.all([
      this.storage.getAllTables(),
      this.storage.getSyncCursor(),
    ]);

    const tableNames = new Set([
      ...Object.keys(this.schema.tables),
      ...Object.keys(tables),
      ...Object.keys(cursor.tables),
    ]);
    const snapshots: Record<string, PyreDevtoolsTableSnapshot> = {};

    Array.from(tableNames).sort().forEach((tableName) => {
      const rows = tables[tableName] ?? [];
      snapshots[tableName] = {
        name: tableName,
        count: rows.length,
        rows,
        sync: this.lastSyncState.tables[tableName],
        cursor: cursor.tables[tableName],
      };
    });

    return {
      schema: this.schema,
      indexedDbName: this.indexedDbName,
      server: {
        baseUrl: this.server.baseUrl,
        endpoints: this.endpoints,
        liveSyncTransport: this.server.liveSyncTransport ?? 'sse',
        hasHeaders: hasServerHeaders(this.server),
        credentials: getServerCredentials(this.server),
        withCredentials: shouldIncludeCredentials(this.server),
      },
      connectionId: this.connectionId,
      syncState: this.lastSyncState,
      syncProgress: this.lastSyncProgress,
      debugValues: { ...this.devtoolsDebugValues },
      tables: snapshots,
    };
  }

  setDevtoolsDebugValue(name: string, value: unknown): void {
    if (value === undefined) {
      delete this.devtoolsDebugValues[name];
    } else {
      this.devtoolsDebugValues[name] = value;
    }
    this.emitDevtoolsEvent('debug:value', { name, value });
  }

  getConnectionId(): string | null {
    return this.connectionId;
  }

  setSession(session: Record<string, unknown> | null): void {
    this.session = session ?? {};
    this.emitDevtoolsEvent('session:update', { keys: Object.keys(this.session) });
    this.queryClient.refreshAllQueries();
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
    this.bridgeCleanup?.();
    this.bridgeCleanup = null;
    this.sseManager.disconnect();
    this.webSocketManager.disconnect();
    this.connectionId = null;
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
    this.emitDevtoolsEvent(`sync:${message.type}`, message);

    if (message.type === 'connected') {
      const connectionId = message.connectionId;
      if (connectionId) {
        this.connectionId = connectionId;
        this.connectionCallbacks.forEach((callback) => {
          callback(connectionId);
        });
      }
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
    this.emitDevtoolsEvent('sync:state', state);
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
    this.emitDevtoolsEvent('indexeddb:delete', { indexedDbName: this.indexedDbName });
  }

  registerQuery<Input = unknown>(
    registration: RegisteredQuery<Input>,
    callback: (result: RegisteredQueryResult) => void
  ): QuerySubscription<Input> {
    const normalizedInput = (registration.input ?? {}) as Input;
    this.emitDevtoolsEvent('query:register', {
      queryId: registration.queryId,
      queryName: registration.queryName,
      input: normalizedInput,
      querySource: registration.querySource,
    });
    this.queryClient.registerQuery(
      {
        queryId: registration.queryId,
        querySource: registration.querySource,
        input: normalizedInput,
      },
      (update) => {
        this.emitDevtoolsEvent('query:result', {
          queryId: update.queryId,
          queryName: registration.queryName,
          revision: update.revision,
          result: update.result,
        });
        callback({
          queryId: update.queryId,
          queryName: registration.queryName,
          revision: update.revision,
          result: update.result,
        });
      }
    );

    return {
      unsubscribe: () => {
        this.emitDevtoolsEvent('query:unregister', { queryId: registration.queryId, queryName: registration.queryName });
        this.queryClient.unregisterQuery(registration.queryId);
      },
      update: (updatedInput: Input) => {
        this.emitDevtoolsEvent('query:update-input', { queryId: registration.queryId, queryName: registration.queryName, input: updatedInput });
        this.queryClient.updateQueryInput(registration.queryId, updatedInput);
      },
    };
  }

  attachElmBridge(config: ElmBridgeConfig): () => void {
    this.bridgeCleanup?.();
    const registrations = new Map<string, QuerySubscription<unknown>>();
    const receivePort = getElmBridgePort(config.app, config.receivePort ?? 'pyreStoreOut');
    const queryResultPort = getElmBridgePort(config.app, config.queryResultPort ?? 'pyre_receiveQueryDelta');
    const syncStatePort = config.syncStatePort
      ? getElmBridgePort(config.app, config.syncStatePort)
      : getElmBridgePort(config.app, 'pyre_receiveSyncState')
      ;
    const mutationResultPort = config.mutationResultPort
      ? getElmBridgePort(config.app, config.mutationResultPort)
      : getElmBridgePort(config.app, 'pyre_receiveMutationResult')
      ;

    const onSyncStateUnsubscribe = syncStatePort?.send
      ? this.onSyncState((syncState) => {
        syncStatePort.send?.({
          status: syncState.status,
          tables: syncState.tables,
          error: syncState.error ? { message: syncState.error } : null,
        });
      })
      : () => {};

    const unsubscribeAll = () => {
      registrations.forEach((subscription) => {
        subscription.unsubscribe();
      });
      registrations.clear();
      onSyncStateUnsubscribe();
      receivePort?.unsubscribe?.(handleIncoming);
      if (this.bridgeCleanup === unsubscribeAll) {
        this.bridgeCleanup = null;
      }
    };

    const handleIncoming = (incoming: unknown) => {
      void (async () => {
        try {
          const message = parseElmBridgeIncomingMessage(incoming);

          if (message.type === 'mutate') {
            if (config.onMutation) {
              try {
                await config.onMutation({ client: this, message });
              } catch (error) {
                reportElmBridgeError(config, error, 'mutation-handler');
              }
            } else {
              this.runBridgeMutation(message, mutationResultPort);
            }
            return;
          }

          if (message.type === 'register') {
            const queryName = asNonEmptyString(message.queryName, 'register message queryName');
            const querySource = asQueryShape(message.querySource);

            registrations.get(message.queryId)?.unsubscribe();

            const subscription = this.registerQuery(
              {
                queryId: message.queryId,
                queryName,
                querySource,
                input: message.queryInput,
              },
              ({ queryId, queryName: resultQueryName, revision, result }) => {
                queryResultPort?.send?.({
                  type: 'full',
                  queryId,
                  queryName: resultQueryName ?? queryName,
                  revision,
                  result,
                });
              }
            );

            registrations.set(message.queryId, subscription as QuerySubscription<unknown>);
            return;
          }

          if (message.type === 'update-input') {
            const registration = registrations.get(message.queryId);
            if (!registration) {
              throw new Error(`update-input for unknown query id: ${message.queryId}`);
            }

            registration.update(message.queryInput);
            return;
          }

          const registration = registrations.get(message.queryId);
          if (!registration) {
            return;
          }

          registration.unsubscribe();
          registrations.delete(message.queryId);
        } catch (error) {
          reportElmBridgeError(config, error, 'incoming-message');
        }
      })();
    };

    receivePort?.subscribe?.(handleIncoming);
    this.bridgeCleanup = unsubscribeAll;
    return unsubscribeAll;
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

    return this.registerQuery(
      {
        queryId,
        querySource: queryShape,
        input: normalizedInput,
      },
      (update) => {
        callback(update.result);
      }
    );
  }

  private runMutation<Input>(
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): void {
    const mutationId = queryModule.id;
    if (!mutationId) {
      throw new Error('Mutation module is missing id');
    }

    const requestId = `mutation_${this.mutationCounter}_${Date.now()}`;
    this.mutationCounter += 1;
    const payload = input ?? {};
    const baseUrl = `${this.server.baseUrl}${this.endpoints.query}`;
    this.emitDevtoolsEvent('mutation:request', { requestId, mutationId, input: payload });
    void (async () => {
      this.queryManager.sendMutation(
        requestId,
        mutationId,
        baseUrl,
        payload,
        (result) => {
          this.emitDevtoolsEvent('mutation:result', { requestId, mutationId, result });
          callback(result);
        },
        await resolveServerHeaders(this.server),
        getServerCredentials(this.server),
        this.server.withCredentials === true
      );
    })();
  }

  private runBridgeMutation(
    message: ElmBridgeMutationMessage,
    mutationResultPort?: ElmBridgePort
  ): void {
    const baseUrl = `${this.server.baseUrl}${this.endpoints.query}`;
    this.emitDevtoolsEvent('mutation:request', {
      requestId: message.requestId,
      mutationId: message.mutationId,
      mutationName: message.mutationName,
      input: message.mutationInput ?? {},
    });
    void (async () => {
      this.queryManager.sendMutation(
        message.requestId,
        message.mutationId,
        baseUrl,
        message.mutationInput ?? {},
        (result) => {
          this.emitDevtoolsEvent('mutation:result', {
            requestId: message.requestId,
            mutationId: message.mutationId,
            mutationName: message.mutationName,
            result,
          });
          mutationResultPort?.send?.({
            type: 'mutation-result',
            requestId: message.requestId,
            mutationId: message.mutationId,
            mutationName: message.mutationName ?? null,
            result,
          } satisfies ElmBridgeMutationResultMessage);
        },
        await resolveServerHeaders(this.server),
        getServerCredentials(this.server),
        this.server.withCredentials === true
      );
    })();
  }

  private emitDevtoolsEvent(type: string, payload?: unknown): void {
    if (this.devtoolsEventCallbacks.size === 0) {
      return;
    }

    const event = {
      id: this.devtoolsEventCounter,
      timestamp: Date.now(),
      type,
      payload,
    } satisfies PyreDevtoolsEvent;
    this.devtoolsEventCounter += 1;

    this.devtoolsEventCallbacks.forEach((callback) => {
      callback(event);
    });
  }
}

function createInitialSyncState(schema: SchemaMetadata): SyncState {
  const tables: Record<string, TableSyncStatus> = {};
  Object.entries(schema.tables)
    .filter(([, table]) => table.sync !== 'query-only')
    .forEach(([tableName]) => {
      tables[tableName] = 'waiting';
    });

  return {
    status: 'not_started',
    tables,
  };
}

function getServerCredentials(server: ServerConfig): RequestCredentials {
  if (server.credentials) {
    return server.credentials;
  }

  return server.withCredentials === true ? 'include' : 'same-origin';
}

function shouldIncludeCredentials(server: ServerConfig): boolean {
  return getServerCredentials(server) === 'include';
}

function getStaticServerHeaders(server: ServerConfig): Record<string, string> {
  if (!server.headers || typeof server.headers === 'function') {
    return {};
  }

  return server.headers;
}

async function resolveServerHeaders(server: ServerConfig): Promise<Record<string, string>> {
  if (!server.headers) {
    return {};
  }

  if (typeof server.headers === 'function') {
    return server.headers();
  }

  return server.headers;
}

function serverHeadersToPairs(headers: Record<string, string>): Array<[string, string]> {
  return Object.entries(headers);
}

function hasServerHeaders(server: ServerConfig): boolean {
  return Boolean(server.headers);
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

function getElmBridgePort(app: ElmBridgeApp, portName: string): ElmBridgePort | undefined {
  return app.ports?.[portName];
}

async function resolveCreateConfig(config: PyreClientCreateConfig): Promise<PyreClientConfig> {
  if ((config.server || config.session) && config.connect) {
    throw new Error('Provide either server/session or connect, not both');
  }

  const connected = config.connect ? await config.connect() : null;
  const server = connected?.server ?? config.server;

  if (!server) {
    throw new Error('PyreClient.create requires server or connect');
  }

  const session = connected?.session ?? config.session;
  const resolvedHeaders = await resolveServerHeaders(server);

  return {
    schema: config.schema,
    server,
    resolvedHeaders,
    indexedDbName: config.indexedDbName,
    onError: config.onError,
    session,
    elm: config.elm,
  };
}

function reportElmBridgeError(
  config: ElmBridgeConfig,
  error: unknown,
  phase: 'incoming-message' | 'mutation-handler'
): void {
  const normalized = error instanceof Error ? error : new Error(String(error));
  if (config.onError) {
    config.onError(normalized, { phase });
    return;
  }

  console.error('[PyreClient] Elm bridge error', phase, normalized);
}

function parseElmBridgeIncomingMessage(message: unknown): ElmBridgeIncomingMessage {
  const raw = asObject(message, 'Pyre bridge message');
  const type = raw.type;

  if (type === 'mutate') {
    return {
      type: 'mutate',
      requestId: asNonEmptyString(raw.requestId, 'mutate message requestId'),
      mutationId: asNonEmptyString(raw.mutationId, 'mutate message mutationId'),
      mutationName: typeof raw.mutationName === 'string' && raw.mutationName.trim() !== '' ? raw.mutationName : undefined,
      mutationInput: raw.mutationInput,
    };
  }

  if (type !== 'register' && type !== 'update-input' && type !== 'unregister') {
    throw new Error(`Unsupported Pyre bridge message type: ${String(type)}`);
  }

  return {
    type,
    queryId: asNonEmptyString(raw.queryId, 'Pyre bridge message queryId'),
    queryName: typeof raw.queryName === 'string' && raw.queryName.trim() !== '' ? raw.queryName : undefined,
    querySource: raw.querySource,
    queryInput: raw.queryInput,
  };
}

function asObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }

  return value as Record<string, unknown>;
}

function asNonEmptyString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`${label} must be non-empty string`);
  }

  return value;
}

function asQueryShape(value: unknown): Record<string, unknown> {
  return asObject(value, 'Pyre bridge querySource');
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
