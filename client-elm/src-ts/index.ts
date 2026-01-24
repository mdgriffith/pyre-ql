import loadElm from '../dist/engine.mjs';
import type { SchemaMetadata } from '../../client/src/types';
import { IndexedDBStorage, IndexedDbService } from './service/indexeddb';
import { QueryManagerService } from './service/query-manager';
import { SSEManager } from './service/sse';
import type { ElmApp, ServerConfig, ServerEndpoints, SyncProgress } from './types';

export type { ServerConfig, ServerEndpoints, SyncProgress } from './types';
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
  private queryManager: QueryManagerService;
  private server: ServerConfig;
  private endpoints: ServerEndpoints;
  private queryCounter = 0;

  constructor(config: PyreClientConfig) {
    const dbName = config.indexedDbName ?? 'pyre-client';
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
        },
      },
    });

    this.storage = new IndexedDBStorage(dbName);
    this.indexedDbService = new IndexedDbService(this.storage);
    this.sseManager = new SSEManager({
      baseUrl: config.server.baseUrl,
      eventsPath: this.endpoints.events,
    });
    this.queryManager = new QueryManagerService();

    this.indexedDbService.attachPorts(this.elmApp);
    this.sseManager.attachPorts(this.elmApp);
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
    return this.sseManager.onSyncProgress(callback);
  }

  onSession(callback: (sessionId: string) => void): () => void {
    return this.sseManager.onSession(callback);
  }

  getSessionId(): string | null {
    return this.sseManager.getSessionId();
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
