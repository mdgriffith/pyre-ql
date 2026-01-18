/**
 * TypeScript bridge for IndexedDB and SSE communication with Elm
 */

// Port interface for Elm app
interface ElmApp {
  ports: {
    // Outgoing ports (Elm -> JS) - organized by service
    indexedDbOut?: {
      subscribe: (callback: (message: any) => void) => void;
    };
    sseOut?: {
      subscribe: (callback: (message: any) => void) => void;
    };
    queryManagerOut?: {
      subscribe: (callback: (message: any) => void) => void;
    };
    errorOut?: {
      subscribe: (callback: (message: string) => void) => void;
    };

    // Incoming ports (JS -> Elm) - namespaced
    receiveIndexedDbMessage?: {
      send: (message: any) => void;
    };
    receiveSSEMessage?: {
      send: (message: any) => void;
    };
    receiveQueryManagerMessage?: {
      send: (message: any) => void;
    };
  };
}

interface TableGroup {
  table_name: string;
  headers: string[];
  rows: any[][];  // Array of row arrays
}

interface InitialData {
  tables: Record<string, any[]>;
}

interface SyncProgress {
  table?: string;
  tablesSynced: number;
  totalTables?: number;
  complete: boolean;
  error?: string;
}

interface SSEConfig {
  baseUrl: string;
  userId: number;
}

const DB_VERSION = 1;
const CURSOR_KEY = 'cursor';

class IndexedDBStorage {
  private dbName: string;
  private db: IDBDatabase | null = null;
  private initPromise: Promise<IDBDatabase> | null = null;

  constructor(dbName: string) {
    this.dbName = dbName;
  }

  async init(): Promise<IDBDatabase> {
    if (this.db) {
      return this.db;
    }

    if (this.initPromise) {
      return this.initPromise;
    }

    this.initPromise = new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, DB_VERSION);

      request.onerror = () => {
        this.initPromise = null;
        reject(new Error(`Failed to open IndexedDB: ${request.error}`));
      };

      request.onsuccess = () => {
        this.db = request.result;
        this.initPromise = null;
        resolve(this.db);
      };

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;

        if (!db.objectStoreNames.contains('tables')) {
          const tablesStore = db.createObjectStore('tables', { keyPath: ['tableName', 'id'] });
          tablesStore.createIndex('byTable', 'tableName', { unique: false });
          tablesStore.createIndex('byUpdatedAt', 'updatedAt', { unique: false });
        }

        if (!db.objectStoreNames.contains('syncCursor')) {
          db.createObjectStore('syncCursor');
        }
      };
    });

    return this.initPromise;
  }

  private async getDB(): Promise<IDBDatabase> {
    if (!this.db) {
      await this.init();
    }
    if (!this.db) {
      throw new Error('Failed to initialize database');
    }
    return this.db;
  }

  async getAllRows(tableName: string): Promise<any[]> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const index = store.index('byTable');
      const range = IDBKeyRange.only(tableName);
      const request = index.getAll(range);

      request.onsuccess = () => {
        const result = request.result || [];
        // Remove tableName from rows
        resolve(result.map(row => {
          const { tableName, ...rest } = row;
          return rest;
        }));
      };

      request.onerror = () => {
        reject(new Error(`Failed to read rows: ${request.error}`));
      };
    });
  }

  async getAllTables(): Promise<Record<string, any[]>> {
    const db = await this.getDB();
    const tables: Record<string, any[]> = {};

    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const request = store.getAll();

      request.onsuccess = () => {
        const allRows = request.result || [];

        // Group by tableName
        for (const row of allRows) {
          const tableName = row.tableName;
          if (!tables[tableName]) {
            tables[tableName] = [];
          }
          const { tableName: _, ...rest } = row;
          tables[tableName].push(rest);
        }

        resolve(tables);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read tables: ${request.error}`));
      };
    });
  }

  async putRows(tableName: string, rows: any[]): Promise<void> {
    if (rows.length === 0) {
      return;
    }

    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readwrite');
      const store = tx.objectStore('tables');

      let error: Error | null = null;
      const existingRows: (any | null)[] = new Array(rows.length);
      let readsCompleted = 0;

      tx.oncomplete = () => {
        if (error) {
          reject(error);
        } else {
          resolve();
        }
      };

      tx.onerror = () => {
        reject(new Error(`Transaction failed: ${tx.error}`));
      };

      // Read existing rows for conflict resolution
      rows.forEach((row, index) => {
        const request = store.get([tableName, row.id]);
        request.onsuccess = () => {
          existingRows[index] = request.result || null;
          readsCompleted++;

          if (readsCompleted === rows.length) {
            processWrites();
          }
        };
        request.onerror = () => {
          existingRows[index] = null;
          readsCompleted++;

          if (readsCompleted === rows.length) {
            processWrites();
          }
        };
      });

      const processWrites = () => {
        rows.forEach((row, index) => {
          const existing = existingRows[index];

          // Conflict resolution: newer updatedAt wins
          if (existing && existing.updatedAt != null && row.updatedAt != null) {
            const existingTime = typeof existing.updatedAt === 'number'
              ? existing.updatedAt
              : new Date(existing.updatedAt).getTime() / 1000;
            const newTime = typeof row.updatedAt === 'number'
              ? row.updatedAt
              : new Date(row.updatedAt).getTime() / 1000;

            if (existingTime > newTime) {
              return; // Skip, existing is newer
            }
          } else if (existing && existing.updatedAt != null && row.updatedAt == null) {
            return; // Skip, existing has updatedAt but new doesn't
          }

          const rowWithTable = { ...row, tableName };
          const request = store.put(rowWithTable);

          request.onerror = () => {
            error = new Error(`Failed to write row: ${request.error}`);
          };
        });
      };
    });
  }
}

class SSEManager {
  private eventSource: EventSource | null = null;
  private sessionId: string | null = null;
  private config: SSEConfig | null = null;
  private elmApp: ElmApp | null = null;
  private clientId: string;

  constructor() {
    this.clientId = `client_${Math.random().toString(36).substring(2, 15)}_${Date.now()}`;
  }

  setElmApp(elmApp: ElmApp) {
    this.elmApp = elmApp;
  }

  connect(config: SSEConfig): void {
    this.config = config;
    this.shouldReconnect = true;
    this.attemptConnect();
  }

  private shouldReconnect = true;

  private attemptConnect(): void {
    if (!this.config) {
      return;
    }

    try {
      const sseUrl = `${this.config.baseUrl}/sync/events?userId=${this.config.userId}&clientId=${encodeURIComponent(this.clientId)}`;
      const eventSource = new EventSource(sseUrl);

      eventSource.onopen = () => {
        this.eventSource = eventSource;
      };

      eventSource.addEventListener('connected', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data);
          if (message.sessionId && this.elmApp?.ports.receiveSSEMessage) {
            this.elmApp.ports.receiveSSEMessage.send({
              type: 'connected',
              sessionId: message.sessionId
            });
          }
        } catch (error) {
          console.error('Failed to parse SSE connected message:', error);
          if (this.elmApp?.ports.receiveSSEMessage) {
            this.elmApp.ports.receiveSSEMessage.send({
              type: 'error',
              error: 'Failed to parse connection message'
            });
          }
        }
      });

      eventSource.addEventListener('delta', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data);
          if (this.elmApp?.ports.receiveSSEMessage) {
            this.elmApp.ports.receiveSSEMessage.send({
              type: 'delta',
              data: message.data
            });
          }
        } catch (error) {
          console.error('Failed to parse SSE delta message:', error);
        }
      });

      eventSource.addEventListener('syncProgress', (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data);
          if (this.elmApp?.ports.receiveSSEMessage) {
            this.elmApp.ports.receiveSSEMessage.send({
              type: 'syncProgress',
              data: message.data
            });
          }
        } catch (error) {
          console.error('Failed to parse SSE sync progress message:', error);
        }
      });

      eventSource.addEventListener('syncComplete', (event: MessageEvent) => {
        if (this.elmApp?.ports.receiveSSEMessage) {
          this.elmApp.ports.receiveSSEMessage.send({
            type: 'syncComplete'
          });
        }
      });

      eventSource.onerror = (error) => {
        const state = eventSource.readyState;

        if (state === EventSource.CLOSED) {
          console.warn('[PyreClient] SSE connection closed');
          if (this.shouldReconnect) {
            // EventSource will auto-reconnect
          }
        } else if (state === EventSource.CONNECTING && !this.sessionId) {
          if (this.elmApp?.ports.receiveSSEMessage) {
            this.elmApp.ports.receiveSSEMessage.send({
              type: 'error',
              error: 'SSE connection failed'
            });
          }
        }
      };
    } catch (error) {
      if (this.elmApp?.ports.receiveSSEMessage) {
        this.elmApp.ports.receiveSSEMessage.send({
          type: 'error',
          error: `SSE connection error: ${error}`
        });
      }
    }
  }

  disconnect(): void {
    this.shouldReconnect = false;

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.sessionId = null;
  }
}

// Global instances
let storage: IndexedDBStorage | null = null;
let sseManager: SSEManager | null = null;

export function initPyreElmClient(elmApp: ElmApp, dbName: string = 'pyre-client'): void {
  storage = new IndexedDBStorage(dbName);
  sseManager = new SSEManager();
  sseManager.setElmApp(elmApp);

  // Handle error port
  if (elmApp.ports.errorOut) {
    elmApp.ports.errorOut.subscribe((message: string) => {
      console.error('[PyreClient]', message);
    });
  }

  // Handle IndexedDB service messages
  if (elmApp.ports.indexedDbOut) {
    elmApp.ports.indexedDbOut.subscribe(async (message: any) => {
      if (message.type === 'requestInitialData') {
        try {
          await storage!.init();
          const tables = await storage!.getAllTables();

          if (elmApp.ports.receiveIndexedDbMessage) {
            elmApp.ports.receiveIndexedDbMessage.send({
              type: 'initialData',
              data: { tables }
            });
          }
        } catch (error) {
          console.error('[PyreClient] Failed to load initial data:', error);
          if (elmApp.ports.receiveIndexedDbMessage) {
            elmApp.ports.receiveIndexedDbMessage.send({
              type: 'initialData',
              data: { tables: {} }
            });
          }
        }
      } else if (message.type === 'writeDelta') {
        try {
          await storage!.init();

          const tableGroups: TableGroup[] = message.tableGroups || [];

          // Write each table group to IndexedDB
          for (const tableGroup of tableGroups) {
            const tableName = tableGroup.table_name;
            if (!tableName) {
              continue;
            }

            // Convert row arrays to row objects using headers
            const rows = tableGroup.rows.map((rowArray: any[]) => {
              const rowObj: Record<string, any> = {};
              tableGroup.headers.forEach((header, i) => {
                rowObj[header] = rowArray[i];
              });
              return rowObj;
            });

            await storage!.putRows(tableName, rows);
          }
        } catch (error) {
          console.error('[PyreClient] Failed to write delta:', error);
        }
      }
    });
  }

  // Handle SSE service messages
  if (elmApp.ports.sseOut) {
    elmApp.ports.sseOut.subscribe((message: any) => {
      if (message.type === 'connectSSE') {
        sseManager!.connect(message.config);
      } else if (message.type === 'disconnectSSE') {
        sseManager!.disconnect();
      }
    });
  }

  // Handle QueryManager service messages (outgoing only - results are sent back via receiveQueryManagerMessage)
  // Query results and mutation results are handled by the Elm app itself
  // This port is mainly for logging/debugging if needed
  if (elmApp.ports.queryManagerOut) {
    elmApp.ports.queryManagerOut.subscribe((message: any) => {
      // Query results and mutation results are sent out but don't need JS handling
      // They're handled by the calling code that registered the query/sent the mutation
      if (message.type === 'queryResult' || message.type === 'mutationResult') {
        // These are handled by the application code that uses the client
        // No action needed here
      }
    });
  }
}
