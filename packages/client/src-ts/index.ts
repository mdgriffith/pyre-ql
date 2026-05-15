import loadElm from '../dist/engine.mjs';
import { IndexedDBStorage, IndexedDbService } from './service/indexeddb';
import { QueryClientService } from './service/query-client';
import { QueryManagerService, type MutationResult } from './service/query-manager';
import { SSEManager, type LiveSyncMessage } from './service/sse';
import { WebSocketManager } from './service/websocket';
import {
  deriveIndexedDbName,
  requireDatabaseId,
  resolveEndpointUrl,
  type CacheNamespace,
  type DatabaseId,
} from './routing';
import {
  registerPyreDevtoolsClient,
  unregisterPyreDevtoolsClient,
} from './devtools-registry';
import type {
  ElmApp,
  LiveSyncTransport,
  SchemaMetadata,
  ServerConfig,
  ServerEndpoints,
  SyncState,
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
export type { CacheNamespace, DatabaseId } from './routing';

interface PyreBridgeClient {
  run<Input = unknown>(
    databaseId: DatabaseId,
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): QuerySubscription<Input> | void | Promise<QuerySubscription<Input> | void>;
}

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
  databaseId: DatabaseId;
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

export type DevtoolsDatabaseLifecycle = 'not_started' | 'queued' | 'syncing' | 'live' | 'unsynced' | 'error';

export interface PyreDevtoolsTableSnapshot {
  name: string;
  count: number;
  rows?: unknown[];
  sync?: TableSyncStatus;
  cursor?: {
    last_seen_updated_at: number | null;
    permission_hash: string;
  };
}

export interface DevtoolsTableSummary {
  name: string;
  count?: number;
  sync?: TableSyncStatus;
  cursor?: {
    last_seen_updated_at: number | null;
    permission_hash: string;
  };
}

export interface DevtoolsDatabaseSummary {
  databaseId: string;
  indexedDbName: string;
  flaggedForSync: boolean;
  lifecycle: DevtoolsDatabaseLifecycle;
  syncState?: SyncState;
  tableSummaries?: DevtoolsTableSummary[];
  error?: string;
}

export interface DevtoolsInstanceSummary {
  instanceId: string;
  label: string;
  cacheNamespace?: string;
}

export interface PyreDevtoolsRegistrySnapshot {
  instances: DevtoolsInstanceSummary[];
}

export interface PyreDevtoolsInstanceSnapshot {
  instanceId: string;
  label: string;
  cacheNamespace?: string;
  schema: SchemaMetadata;
  server: PyreDevtoolsSnapshot['server'];
  aggregateSyncState: SyncState;
  syncProgress: SyncProgress | null;
  debugValues: Record<string, unknown>;
  databases: DevtoolsDatabaseSummary[];
  events: PyreDevtoolsEvent[];
}

export interface PyreDevtoolsTablePageRequest {
  instanceId: string;
  databaseId: string;
  tableName: string;
  offset?: number;
  limit?: number;
  filter?: unknown;
  sort?: unknown;
}

export interface PyreDevtoolsTablePage {
  rows: unknown[];
  offset: number;
  limit: number;
  hasMore: boolean;
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
  databaseId: DatabaseId;
  queryId: string;
  queryName?: string;
  querySource?: unknown;
  queryInput?: unknown;
}

export interface ElmBridgeMutationMessage {
  type: 'mutate';
  databaseId: DatabaseId;
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
    client: PyreBridgeClient;
    message: ElmBridgeMutationMessage;
  }) => Promise<void> | void;
  onError?: (error: Error, context: { phase: 'incoming-message' | 'mutation-handler' }) => void;
}

export interface PyreElmConfig extends ElmBridgeConfig {}

interface SingleDatabasePyreClientConfig {
  schema: SchemaMetadata;
  server: ServerConfig;
  databaseId?: DatabaseId;
  cacheNamespace?: CacheNamespace;
  autoStartSync?: boolean;
  resolvedHeaders?: Record<string, string>;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  elm?: PyreElmConfig;
}

interface SingleDatabasePyreClientCreateConfig {
  schema: SchemaMetadata;
  server?: ServerConfig;
  databaseId?: DatabaseId;
  cacheNamespace?: CacheNamespace;
  autoStartSync?: boolean;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  connect?: () => Promise<{
    server: ServerConfig;
    databaseId?: DatabaseId;
    cacheNamespace?: CacheNamespace;
    session?: Record<string, unknown>;
  }> | {
    server: ServerConfig;
    databaseId?: DatabaseId;
    cacheNamespace?: CacheNamespace;
    session?: Record<string, unknown>;
  };
  elm?: PyreElmConfig;
}

export type PyreInternalClient = Pick<SingleDatabasePyreClient, 'run' | 'disconnect' | 'getDevtoolsSnapshot' | 'inspectDevtoolsTablePage' | 'startSync' | 'onSyncState' | 'setSession'>;

type PyreInternalClientFactory = (config: SingleDatabasePyreClientCreateConfig & {
  databaseId: DatabaseId;
  cacheNamespace: CacheNamespace;
}) => Promise<PyreInternalClient>;

export interface PyreClientConfig {
  schema: SchemaMetadata;
  server?: ServerConfig;
  cacheNamespace?: CacheNamespace;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  connect?: () => Promise<{
    server: ServerConfig;
    cacheNamespace?: CacheNamespace;
    session?: Record<string, unknown>;
  }> | {
    server: ServerConfig;
    cacheNamespace?: CacheNamespace;
    session?: Record<string, unknown>;
  };
  createInternalClient?: PyreInternalClientFactory;
  elm?: PyreElmConfig;
}

interface ResolvedPyreClientConfig {
  schema: SchemaMetadata;
  server: ServerConfig;
  cacheNamespace: CacheNamespace;
  indexedDbName?: string;
  debug?: boolean;
  onError?: (error: Error) => void;
  session?: Record<string, unknown>;
  createInternalClient: PyreInternalClientFactory;
  elm?: PyreElmConfig;
}

class SingleDatabasePyreClient {
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

  constructor(config: SingleDatabasePyreClientConfig) {
    const baseIndexedDbName = config.indexedDbName ?? 'pyre-client';
    const dbName = config.cacheNamespace && config.databaseId
      ? deriveIndexedDbName(baseIndexedDbName, config.cacheNamespace, config.databaseId)
      : baseIndexedDbName;
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
            databaseId: config.databaseId ?? null,
            headers: serverHeadersToPairs(initialHeaders),
            credentials: getServerCredentials(config.server),
            withCredentials: shouldIncludeCredentials(config.server),
          },
          liveSync: {
            transport: liveSyncTransport,
          },
          sync: {
            autoStart: config.autoStartSync ?? true,
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
      databaseId: config.databaseId,
    }, undefined, this.logDebug);
    this.webSocketManager = new WebSocketManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
      databaseId: config.databaseId,
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

  static async create(config: SingleDatabasePyreClientCreateConfig): Promise<SingleDatabasePyreClient> {
    const resolvedConfig = await resolveCreateConfig(config);
    const client = new SingleDatabasePyreClient(resolvedConfig);
    await client.init();
    if (resolvedConfig.elm) {
      client.bridgeCleanup = client.attachElmBridge(resolvedConfig.elm);
    }
    return client;
  }

  async init(): Promise<void> {
    await this.storage.init();
  }

  startSync(): void {
    this.elmApp.ports.receiveSyncControlMessage?.send({ type: 'startSync' });
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
    const cursor = await this.storage.getSyncCursor();

    const tableNames = new Set([
      ...Object.keys(this.schema.tables),
      ...Object.keys(cursor.tables),
    ]);
    const snapshots: Record<string, PyreDevtoolsTableSnapshot> = {};

    await Promise.all(Array.from(tableNames).sort().map(async (tableName) => {
      snapshots[tableName] = {
        name: tableName,
        count: await this.storage.countRows(tableName),
        sync: this.lastSyncState.tables[tableName],
        cursor: cursor.tables[tableName],
      };
    }));

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

  async inspectDevtoolsTablePage(request: Omit<PyreDevtoolsTablePageRequest, 'instanceId' | 'databaseId'>): Promise<PyreDevtoolsTablePage> {
    const offset = Math.max(0, Math.floor(request.offset ?? 0));
    const limit = Math.max(1, Math.min(500, Math.floor(request.limit ?? 100)));
    const page = await this.storage.getRowsPage(request.tableName, offset, limit);
    return {
      rows: page.rows,
      offset,
      limit,
      hasMore: page.hasMore,
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
    databaseId: DatabaseId,
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): QuerySubscription<Input> | void {
    const targetDatabaseId = requireDatabaseId(databaseId);

    if (queryModule.operation === 'query') {
      return this.runQuery(targetDatabaseId, queryModule, input, callback);
    }

    this.runMutation(targetDatabaseId, queryModule, input, callback);
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
                databaseId: message.databaseId,
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
    databaseId: DatabaseId,
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
        databaseId,
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
    databaseId: DatabaseId,
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
    const baseUrl = resolveEndpointUrl(this.server.baseUrl, this.endpoints.query, { databaseId });
    this.emitDevtoolsEvent('mutation:request', { requestId, mutationId, databaseId, input: payload });
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
    const baseUrl = resolveEndpointUrl(this.server.baseUrl, this.endpoints.query, {
      databaseId: message.databaseId,
    });
    this.emitDevtoolsEvent('mutation:request', {
      requestId: message.requestId,
      mutationId: message.mutationId,
      mutationName: message.mutationName,
      databaseId: message.databaseId,
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

export class PyreClient {
  private static readonly devtoolsEventLimit = 200;
  private config: ResolvedPyreClientConfig;
  private clients: Map<DatabaseId, Promise<PyreInternalClient>> = new Map();
  private clientGenerations: Map<DatabaseId, number> = new Map();
  private knownDatabaseIds: DatabaseId[] = [];
  private syncedDatabaseIds: DatabaseId[] = [];
  private syncingDatabaseId: DatabaseId | null = null;
  private completedSyncDatabaseIds: Set<DatabaseId> = new Set();
  private syncStateCallbacks: Set<(state: SyncState) => void> = new Set();
  private latestSyncStates: Map<DatabaseId, SyncState> = new Map();
  private bridgeCleanup: (() => void) | null = null;
  private devtoolsDebugValues: Record<string, unknown> = {};
  private devtoolsEventCounter = 0;
  private devtoolsEvents: PyreDevtoolsEvent[] = [];
  private devtoolsEventCallbacks: Set<(event: PyreDevtoolsEvent) => void> = new Set();
  private instanceId: string;

  private constructor(config: ResolvedPyreClientConfig) {
    this.config = config;
    this.instanceId = registerPyreDevtoolsClient(this);
  }

  static async create(config: PyreClientConfig): Promise<PyreClient> {
    const resolved = await resolveMultiCreateConfig(config);
    const client = new PyreClient(resolved);
    try {
      if (resolved.elm) {
        client.attachElmBridge(resolved.elm);
      }
    } catch (error) {
      client.disconnect();
      throw error;
    }
    return client;
  }

  run<Input = unknown>(
    databaseId: DatabaseId,
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): Promise<QuerySubscription<Input> | void> {
    this.markKnownDatabase(databaseId);
    if (queryModule.operation === 'mutation') {
      return this.runPublicMutation(databaseId, queryModule, input, callback);
    }
    return this.getOrCreateClient(databaseId).then((client) => (
      client.run(databaseId, queryModule, input, callback)
    ));
  }

  async getOrCreateClient(databaseId: DatabaseId): Promise<PyreInternalClient> {
    const targetDatabaseId = requireDatabaseId(databaseId);
    this.markKnownDatabase(targetDatabaseId);
    const existing = this.clients.get(targetDatabaseId);
    if (existing) {
      return existing;
    }

    const generation = (this.clientGenerations.get(targetDatabaseId) ?? 0) + 1;
    this.clientGenerations.set(targetDatabaseId, generation);

    const created = this.config.createInternalClient(this.internalClientConfig(targetDatabaseId));
    this.clients.set(targetDatabaseId, created);
    void created.then((client) => {
      this.watchInternalClient(targetDatabaseId, generation, client);
    });
    return created;
  }

  async setSyncedDatabases(databaseIds: DatabaseId[]): Promise<void> {
    const nextDatabaseIds = dedupeDatabaseIds(databaseIds);
    nextDatabaseIds.forEach((databaseId) => this.markKnownDatabase(databaseId));
    const nextSet = new Set(nextDatabaseIds);

    const removedDatabaseIds = this.syncedDatabaseIds.filter((databaseId) => !nextSet.has(databaseId));
    removedDatabaseIds.forEach((databaseId) => {
      this.completedSyncDatabaseIds.delete(databaseId);
      if (this.syncingDatabaseId === databaseId) {
        this.syncingDatabaseId = null;
      }
      this.disconnectInternalClient(databaseId);
    });

    this.syncedDatabaseIds = nextDatabaseIds;
    this.emitDevtoolsEvent('sync.databases', {
      instanceId: this.instanceId,
      databaseIds: [...this.syncedDatabaseIds],
    });

    for (const databaseId of nextDatabaseIds) {
      await this.getOrCreateClient(databaseId);
    }

    this.startNextSync();
  }

  async syncDatabase(databaseId: DatabaseId): Promise<void> {
    const targetDatabaseId = requireDatabaseId(databaseId);
    this.markKnownDatabase(targetDatabaseId);
    if (this.syncedDatabaseIds.includes(targetDatabaseId)) {
      return;
    }

    await this.setSyncedDatabases([...this.syncedDatabaseIds, targetDatabaseId]);
  }

  async unsyncDatabase(databaseId: DatabaseId): Promise<void> {
    const targetDatabaseId = requireDatabaseId(databaseId);
    this.markKnownDatabase(targetDatabaseId);
    await this.setSyncedDatabases(this.syncedDatabaseIds.filter((id) => id !== targetDatabaseId));
  }

  getInternalIndexedDbName(databaseId: DatabaseId): string {
    return deriveIndexedDbName(
      this.config.indexedDbName ?? 'pyre-client',
      this.config.cacheNamespace,
      databaseId
    );
  }

  getInternalDatabaseIds(): DatabaseId[] {
    return Array.from(this.clients.keys());
  }

  getKnownDatabaseIds(): DatabaseId[] {
    return [...this.knownDatabaseIds];
  }

  getSyncedDatabaseIds(): DatabaseId[] {
    return [...this.syncedDatabaseIds];
  }

  onSyncState(callback: (state: SyncState) => void): () => void {
    this.syncStateCallbacks.add(callback);
    callback(this.aggregateSyncState());
    return () => {
      this.syncStateCallbacks.delete(callback);
    };
  }

  onSyncProgress(callback: (progress: SyncProgress) => void): () => void {
    return this.onSyncState((state) => {
      callback(toSyncProgress(state));
    });
  }

  onDevtoolsEvent(callback: (event: PyreDevtoolsEvent) => void): () => void {
    this.devtoolsEventCallbacks.add(callback);
    return () => {
      this.devtoolsEventCallbacks.delete(callback);
    };
  }

  async getDevtoolsSnapshot(): Promise<PyreDevtoolsSnapshot> {
    const selectedDatabaseId = this.knownDatabaseIds[0];
    const firstClient = selectedDatabaseId ? await this.clients.get(selectedDatabaseId) : undefined;
    if (firstClient) {
      const snapshot = await firstClient.getDevtoolsSnapshot();
      return {
        ...snapshot,
        indexedDbName: this.syncedDatabaseIds.length === 1
          ? snapshot.indexedDbName
          : this.config.indexedDbName ?? 'pyre-client',
        syncState: this.aggregateSyncState(),
        syncProgress: toSyncProgress(this.aggregateSyncState()),
        debugValues: { ...snapshot.debugValues, ...this.devtoolsDebugValues },
      };
    }

    const endpoints = {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
      ...this.config.server.endpoints,
    };

    return {
      schema: this.config.schema,
      indexedDbName: this.config.indexedDbName ?? 'pyre-client',
      server: {
        baseUrl: this.config.server.baseUrl,
        endpoints,
        liveSyncTransport: this.config.server.liveSyncTransport ?? 'sse',
        hasHeaders: hasServerHeaders(this.config.server),
        credentials: getServerCredentials(this.config.server),
        withCredentials: shouldIncludeCredentials(this.config.server),
      },
      connectionId: null,
      syncState: this.aggregateSyncState(),
      syncProgress: toSyncProgress(this.aggregateSyncState()),
      debugValues: { ...this.devtoolsDebugValues },
      tables: {},
    };
  }

  async getDevtoolsInstanceSnapshot(): Promise<PyreDevtoolsInstanceSnapshot> {
    const aggregateSyncState = this.aggregateSyncState();
    const databases = await Promise.all(this.knownDatabaseIds.map(async (databaseId) => {
      const client = await this.clients.get(databaseId);
      const syncState = this.latestSyncStates.get(databaseId);
      let tableSummaries: DevtoolsTableSummary[] | undefined;
      if (client) {
        const snapshot = await client.getDevtoolsSnapshot();
        tableSummaries = Object.values(snapshot.tables).map((table) => ({
          name: table.name,
          count: table.count,
          sync: table.sync,
          cursor: table.cursor,
        }));
      }

      return {
        databaseId,
        indexedDbName: this.getInternalIndexedDbName(databaseId),
        flaggedForSync: this.syncedDatabaseIds.includes(databaseId),
        lifecycle: this.classifyDatabaseLifecycle(databaseId),
        syncState,
        tableSummaries,
        error: syncState?.error,
      } satisfies DevtoolsDatabaseSummary;
    }));

    return {
      instanceId: this.instanceId,
      label: this.config.cacheNamespace,
      cacheNamespace: this.config.cacheNamespace,
      schema: this.config.schema,
      server: this.devtoolsServerSnapshot(),
      aggregateSyncState,
      syncProgress: toSyncProgress(aggregateSyncState),
      debugValues: { ...this.devtoolsDebugValues },
      databases,
      events: [...this.devtoolsEvents],
    };
  }

  async inspectDevtoolsTablePage(request: Omit<PyreDevtoolsTablePageRequest, 'instanceId'>): Promise<PyreDevtoolsTablePage> {
    this.markKnownDatabase(request.databaseId);
    const client = await this.getOrCreateClient(request.databaseId);
    return client.inspectDevtoolsTablePage({
      tableName: request.tableName,
      offset: request.offset,
      limit: request.limit,
      filter: request.filter,
      sort: request.sort,
    });
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
    return null;
  }

  setSession(session: Record<string, unknown> | null): void {
    this.config.session = session ?? {};
    this.clients.forEach((clientPromise) => {
      void clientPromise.then((client) => {
        client.setSession(session);
      });
    });
  }

  attachElmBridge(config: ElmBridgeConfig): () => void {
    this.bridgeCleanup?.();
    const registrations = new Map<string, Promise<QuerySubscription<unknown> | void>>();
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
      registrations.forEach((subscriptionPromise) => {
        void subscriptionPromise.then((subscription) => {
          subscription?.unsubscribe();
        });
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
              this.markKnownDatabase(message.databaseId);
              this.emitDevtoolsEvent('mutation.custom_dispatched', {
                instanceId: this.instanceId,
                databaseId: message.databaseId,
                mutationId: message.mutationId,
                mutationName: message.mutationName,
                input: message.mutationInput ?? {},
              });
              try {
                await config.onMutation({ client: this, message });
              } catch (error) {
                reportElmBridgeError(config, error, 'mutation-handler');
              }
            } else {
              await this.run(
                message.databaseId,
                { operation: 'mutation', id: message.mutationId },
                message.mutationInput ?? {},
                (result) => {
                  mutationResultPort?.send?.({
                    type: 'mutation-result',
                    requestId: message.requestId,
                    mutationId: message.mutationId,
                    mutationName: message.mutationName ?? null,
                    result: result as MutationResult,
                  } satisfies ElmBridgeMutationResultMessage);
                }
              );
            }
            return;
          }

          if (message.type === 'register') {
            const queryName = asNonEmptyString(message.queryName, 'register message queryName');
            const querySource = asQueryShape(message.querySource);

            const existingRegistration = registrations.get(message.queryId);
            if (existingRegistration) {
              void existingRegistration.then((subscription) => subscription?.unsubscribe());
            }

            const subscriptionPromise = Promise.resolve(this.run(
              message.databaseId,
              {
                operation: 'query',
                queryShape: querySource,
              },
              message.queryInput ?? {},
              (result) => {
                queryResultPort?.send?.({
                  type: 'full',
                  queryId: message.queryId,
                  queryName,
                  revision: Date.now(),
                  result,
                });
              }
            ));

            registrations.set(message.queryId, subscriptionPromise);
            return;
          }

          if (message.type === 'update-input') {
            const registration = registrations.get(message.queryId);
            if (!registration) {
              throw new Error(`update-input for unknown query id: ${message.queryId}`);
            }

            const subscription = await registration;
            subscription?.update(message.queryInput);
            return;
          }

          const registration = registrations.get(message.queryId);
          if (!registration) {
            return;
          }

          const subscription = await registration;
          subscription?.unsubscribe();
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

  disconnect(): void {
    unregisterPyreDevtoolsClient(this.instanceId);
    this.bridgeCleanup?.();
    this.bridgeCleanup = null;
    this.clients.forEach((clientPromise) => {
      void clientPromise.then((client) => {
        client.disconnect();
      });
    });
    this.clients.clear();
    this.clientGenerations.clear();
    this.knownDatabaseIds = [];
    this.syncedDatabaseIds = [];
    this.syncingDatabaseId = null;
    this.completedSyncDatabaseIds.clear();
    this.latestSyncStates.clear();
    this.emitSyncState();
  }

  private startNextSync(): void {
    if (this.syncingDatabaseId) {
      return;
    }

    const nextDatabaseId = this.syncedDatabaseIds.find((databaseId) => !this.completedSyncDatabaseIds.has(databaseId));
    if (!nextDatabaseId) {
      return;
    }

    const clientPromise = this.clients.get(nextDatabaseId);
    if (!clientPromise) {
      return;
    }

    this.syncingDatabaseId = nextDatabaseId;
    this.emitDevtoolsEvent('sync.scheduler', {
      instanceId: this.instanceId,
      syncingDatabaseId: this.syncingDatabaseId,
      queuedDatabaseIds: this.syncedDatabaseIds.filter((databaseId) => databaseId !== this.syncingDatabaseId && !this.completedSyncDatabaseIds.has(databaseId)),
    });
    void clientPromise.then((client) => {
      if (this.syncingDatabaseId !== nextDatabaseId || !this.syncedDatabaseIds.includes(nextDatabaseId)) {
        return;
      }

      client.startSync();
    });
  }

  private async runPublicMutation<Input>(
    databaseId: DatabaseId,
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): Promise<void> {
    const targetDatabaseId = requireDatabaseId(databaseId);
    const mutationId = queryModule.id;
    if (!mutationId) {
      throw new Error('Mutation module is missing id');
    }

    const startedAt = Date.now();
    const eventPayload = {
      instanceId: this.instanceId,
      databaseId: targetDatabaseId,
      mutationId,
      mutationName: mutationId,
      input: input ?? {},
    };
    this.emitDevtoolsEvent('mutation.started', eventPayload);
    const client = await this.getOrCreateClient(targetDatabaseId);
    client.run(targetDatabaseId, queryModule, input, (result) => {
      const elapsedMs = Date.now() - startedAt;
      const mutationResult = result as MutationResult | undefined;
      if (mutationResult && mutationResult.ok === false) {
        this.emitDevtoolsEvent('mutation.failed', {
          ...eventPayload,
          error: mutationResult.error ?? 'Mutation failed',
          result,
          elapsedMs,
        });
      } else {
        this.emitDevtoolsEvent('mutation.completed', {
          ...eventPayload,
          result,
          elapsedMs,
        });
      }
      callback(result);
    });
  }

  private watchInternalClient(databaseId: DatabaseId, generation: number, client: PyreInternalClient): void {
    this.markKnownDatabase(databaseId);
    client.onSyncState((state) => {
      if (this.clientGenerations.get(databaseId) !== generation) {
        return;
      }

      this.latestSyncStates.set(databaseId, state);
      this.emitDevtoolsEvent('sync.database_state', {
        instanceId: this.instanceId,
        databaseId,
        syncState: state,
      });
      this.emitSyncState();

      if (!this.syncedDatabaseIds.includes(databaseId)) {
        return;
      }

      if (state.status !== 'live') {
        return;
      }

      this.completedSyncDatabaseIds.add(databaseId);
      if (this.syncingDatabaseId === databaseId) {
        this.syncingDatabaseId = null;
      }

      this.startNextSync();
    });
  }

  private disconnectInternalClient(databaseId: DatabaseId): void {
    const clientPromise = this.clients.get(databaseId);
    this.clients.delete(databaseId);
    this.latestSyncStates.delete(databaseId);
    this.emitSyncState();
    this.clientGenerations.set(databaseId, (this.clientGenerations.get(databaseId) ?? 0) + 1);
    if (!clientPromise) {
      return;
    }

    void clientPromise.then((client) => {
      client.disconnect();
    });
  }

  private internalClientConfig(databaseId: DatabaseId): SingleDatabasePyreClientCreateConfig & {
    databaseId: DatabaseId;
    cacheNamespace: CacheNamespace;
  } {
    return {
      schema: this.config.schema,
      server: this.config.server,
      databaseId,
      cacheNamespace: this.config.cacheNamespace,
      autoStartSync: false,
      indexedDbName: this.config.indexedDbName,
      debug: this.config.debug,
      onError: this.config.onError,
      session: this.config.session,
    };
  }

  private emitSyncState(): void {
    if (this.syncStateCallbacks.size === 0) {
      return;
    }

    const state = this.aggregateSyncState();
    this.syncStateCallbacks.forEach((callback) => {
      callback(state);
    });
  }

  private emitDevtoolsEvent(type: string, payload?: unknown): void {
    const event = {
      id: this.devtoolsEventCounter,
      timestamp: Date.now(),
      type,
      payload,
    } satisfies PyreDevtoolsEvent;
    this.devtoolsEventCounter += 1;
    this.devtoolsEvents = [event, ...this.devtoolsEvents].slice(0, PyreClient.devtoolsEventLimit);

    this.devtoolsEventCallbacks.forEach((callback) => {
      callback(event);
    });
  }

  private markKnownDatabase(databaseId: DatabaseId): void {
    const targetDatabaseId = requireDatabaseId(databaseId);
    if (!this.knownDatabaseIds.includes(targetDatabaseId)) {
      this.knownDatabaseIds.push(targetDatabaseId);
      this.emitDevtoolsEvent('database.known', {
        instanceId: this.instanceId,
        databaseId: targetDatabaseId,
      });
    }
  }

  private classifyDatabaseLifecycle(databaseId: DatabaseId): DevtoolsDatabaseLifecycle {
    const syncState = this.latestSyncStates.get(databaseId);
    if (syncState?.error) {
      return 'error';
    }
    if (!this.syncedDatabaseIds.includes(databaseId)) {
      return 'unsynced';
    }
    if (this.syncingDatabaseId === databaseId) {
      return 'syncing';
    }
    if (this.completedSyncDatabaseIds.has(databaseId)) {
      return 'live';
    }
    if (this.syncedDatabaseIds.includes(databaseId)) {
      return 'queued';
    }
    return 'not_started';
  }

  private devtoolsServerSnapshot(): PyreDevtoolsSnapshot['server'] {
    const endpoints = {
      catchup: '/sync',
      events: '/sync/events',
      query: '/db',
      ...this.config.server.endpoints,
    };
    return {
      baseUrl: this.config.server.baseUrl,
      endpoints,
      liveSyncTransport: this.config.server.liveSyncTransport ?? 'sse',
      hasHeaders: hasServerHeaders(this.config.server),
      credentials: getServerCredentials(this.config.server),
      withCredentials: shouldIncludeCredentials(this.config.server),
    };
  }

  private aggregateSyncState(): SyncState {
    const activeStates = this.syncedDatabaseIds
      .map((databaseId) => this.latestSyncStates.get(databaseId))
      .filter((state): state is SyncState => Boolean(state));

    if (activeStates.length === 0) {
      return createInitialSyncState(this.config.schema);
    }

    const tables: Record<string, TableSyncStatus> = {};
    Object.keys(this.config.schema.tables).forEach((tableName) => {
      const tableStates = activeStates.map((state) => state.tables[tableName] ?? 'waiting');
      if (tableStates.every((status) => status === 'live')) {
        tables[tableName] = 'live';
      } else if (tableStates.some((status) => status === 'catching_up')) {
        tables[tableName] = 'catching_up';
      } else {
        tables[tableName] = 'waiting';
      }
    });

    const errorState = activeStates.find((state) => state.error);
    return {
      status: activeStates.every((state) => state.status === 'live') ? 'live' : 'catching_up',
      tables,
      error: errorState?.error,
    };
  }
}

function dedupeDatabaseIds(databaseIds: DatabaseId[]): DatabaseId[] {
  const seen = new Set<DatabaseId>();
  const result: DatabaseId[] = [];

  databaseIds.forEach((databaseId) => {
    const targetDatabaseId = requireDatabaseId(databaseId);
    if (seen.has(targetDatabaseId)) {
      return;
    }

    seen.add(targetDatabaseId);
    result.push(targetDatabaseId);
  });

  return result;
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

async function resolveCreateConfig(config: SingleDatabasePyreClientCreateConfig): Promise<SingleDatabasePyreClientConfig> {
  if ((config.server || config.session) && config.connect) {
    throw new Error('Provide either server/session or connect, not both');
  }

  const connected = config.connect ? await config.connect() : null;
  const server = connected?.server ?? config.server;

  if (!server) {
    throw new Error('SingleDatabasePyreClient.create requires server or connect');
  }

  const session = connected?.session ?? config.session;
  const databaseId = connected?.databaseId ?? config.databaseId;
  const cacheNamespace = connected?.cacheNamespace ?? config.cacheNamespace;
  const resolvedHeaders = await resolveServerHeaders(server);

  return {
    schema: config.schema,
    server,
    databaseId,
    cacheNamespace,
    autoStartSync: config.autoStartSync,
    resolvedHeaders,
    indexedDbName: config.indexedDbName,
    onError: config.onError,
    session,
    elm: config.elm,
  };
}

async function resolveMultiCreateConfig(config: PyreClientConfig): Promise<ResolvedPyreClientConfig> {
  if ((config.server || config.session || config.cacheNamespace) && config.connect) {
    throw new Error('Provide either server/session/cacheNamespace or connect, not both');
  }

  const connected = config.connect ? await config.connect() : null;
  const server = connected?.server ?? config.server;

  if (!server) {
    throw new Error('PyreClient.create requires server or connect');
  }

  const cacheNamespace = connected?.cacheNamespace ?? config.cacheNamespace;
  if (!cacheNamespace) {
    throw new Error('PyreClient.create requires cacheNamespace');
  }

  return {
    schema: config.schema,
    server,
    cacheNamespace,
    indexedDbName: config.indexedDbName,
    debug: config.debug,
    onError: config.onError,
    session: connected?.session ?? config.session,
    createInternalClient: config.createInternalClient ?? ((internalConfig) => SingleDatabasePyreClient.create(internalConfig)),
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
      databaseId: requireDatabaseId(raw.databaseId, 'mutate message databaseId'),
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
    databaseId: requireDatabaseId(raw.databaseId, 'Pyre bridge message databaseId'),
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
