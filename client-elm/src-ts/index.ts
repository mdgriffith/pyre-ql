import loadElm from '../dist/engine.mjs';
import type { SchemaMetadata } from '../../client/src/types';
import { IndexedDBStorage, IndexedDbService } from './service/indexeddb';
import { QueryManagerService } from './service/query-manager';
import { SSEManager, type LiveSyncMessage } from './service/sse';
import { WebSocketManager } from './service/websocket';
import type { ElmApp, LiveSyncTransport, ServerConfig, ServerEndpoints, SyncProgress } from './types';

export type { LiveSyncTransport, ServerConfig, ServerEndpoints, SyncProgress } from './types';
export type { MutationResult } from './service/query-manager';

export interface QueryModule<Input = unknown> {
  operation: 'query' | 'insert' | 'update' | 'delete' | 'mutation';
  hash?: string;
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
  private syncProgressCallbacks: Set<(progress: SyncProgress) => void> = new Set();
  private sessionCallbacks: Set<(sessionId: string) => void> = new Set();
  private lastSyncProgress: SyncProgress | null = null;
  private queryManager: QueryManagerService;
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

    const Elm = loadElm(Object.create(globalThis));

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

    this.indexedDbService.attachPorts(this.elmApp);
    this.sseManager.setOnMessage(this.handleLiveSyncMessage);
    this.webSocketManager.setOnMessage(this.handleLiveSyncMessage);
    this.sseManager.attachPorts(this.elmApp);
    this.webSocketManager.attachPorts(this.elmApp);
    this.queryManager.attachPorts(this.elmApp);

    if (this.elmApp.ports.errorOut) {
      this.elmApp.ports.errorOut.subscribe((message) => {
        console.log('[PyreClient] port errorOut <-', message);
        const error = new Error(message);
        if (config.onError) {
          config.onError(error);
        } else {
          console.error('[PyreClient]', message);
        }
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
  }

  private handleLiveSyncMessage = (message: LiveSyncMessage): void => {
    if (message.type === 'connected' && message.sessionId) {
      this.sessionId = message.sessionId;
      this.sessionCallbacks.forEach((callback) => {
        callback(message.sessionId!);
      });
    }

    if (message.type === 'syncProgress' && message.data) {
      const progress = message.data as SyncProgress;
      this.lastSyncProgress = progress;
      this.syncProgressCallbacks.forEach((callback) => {
        callback(progress);
      });
    }

    if (message.type === 'syncComplete' && this.lastSyncProgress) {
      const progress = { ...this.lastSyncProgress, complete: true };
      this.lastSyncProgress = progress;
      this.syncProgressCallbacks.forEach((callback) => {
        callback(progress);
      });
    }
  };

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
    const callbackPort = `callback_${queryId}`;

    this.queryManager.registerQuery(
      {
        queryId,
        query: queryShape,
        input: normalizedInput,
        callbackPort,
      },
      callback
    );

    return {
      unsubscribe: () => {
        this.queryManager.unregisterQuery(queryId, callbackPort);
      },
      update: (updatedInput: Input) => {
        const updatedQuery = queryModule.toQueryShape
          ? queryModule.toQueryShape(updatedInput)
          : queryModule.queryShape;
        this.queryManager.updateQueryInput(queryId, updatedInput, updatedQuery);
      },
    };
  }

  private runMutation<Input>(
    queryModule: QueryModule<Input>,
    input: Input,
    callback: (result: unknown) => void
  ): void {
    const hash = queryModule.hash;
    if (!hash) {
      throw new Error('Mutation module is missing hash');
    }

    const payload = input ?? {};
    const baseUrl = `${this.server.baseUrl}${this.endpoints.query}`;
    this.queryManager.sendMutation(
      hash,
      baseUrl,
      payload,
      callback,
      this.server.headers
    );
  }
}
