import type { ElmApp } from '../types';

export interface TableGroup {
  table_name: string;
  headers: string[];
  rows: unknown[][];
}

const DB_VERSION = 1;

export class IndexedDBStorage {
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

  async getAllRows(tableName: string): Promise<unknown[]> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const index = store.index('byTable');
      const range = IDBKeyRange.only(tableName);
      const request = index.getAll(range);

      request.onsuccess = () => {
        const result = request.result || [];
        resolve(result.map((row) => {
          const { tableName, ...rest } = row as { tableName: string };
          return rest;
        }));
      };

      request.onerror = () => {
        reject(new Error(`Failed to read rows: ${request.error}`));
      };
    });
  }

  async getAllTables(): Promise<Record<string, unknown[]>> {
    const db = await this.getDB();
    const tables: Record<string, unknown[]> = {};

    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const request = store.getAll();

      request.onsuccess = () => {
        const allRows = request.result || [];

        for (const row of allRows) {
          const tableName = (row as { tableName: string }).tableName;
          if (!tables[tableName]) {
            tables[tableName] = [];
          }
          const { tableName: _, ...rest } = row as { tableName: string };
          tables[tableName].push(rest);
        }

        resolve(tables);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read tables: ${request.error}`));
      };
    });
  }

  async putRows(tableName: string, rows: Array<Record<string, unknown>>): Promise<void> {
    if (rows.length === 0) {
      return;
    }

    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readwrite');
      const store = tx.objectStore('tables');

      let error: Error | null = null;
      const existingRows: (Record<string, unknown> | null)[] = new Array(rows.length);
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

      rows.forEach((row, index) => {
        const request = store.get([tableName, row.id as IDBValidKey]);
        request.onsuccess = () => {
          existingRows[index] = request.result || null;
          readsCompleted += 1;

          if (readsCompleted === rows.length) {
            processWrites();
          }
        };
        request.onerror = () => {
          existingRows[index] = null;
          readsCompleted += 1;

          if (readsCompleted === rows.length) {
            processWrites();
          }
        };
      });

      const processWrites = () => {
        rows.forEach((row, index) => {
          const existing = existingRows[index];

          if (existing && existing.updatedAt != null && row.updatedAt != null) {
            const existingTime = typeof existing.updatedAt === 'number'
              ? existing.updatedAt
              : new Date(existing.updatedAt as string).getTime() / 1000;
            const newTime = typeof row.updatedAt === 'number'
              ? row.updatedAt
              : new Date(row.updatedAt as string).getTime() / 1000;

            if (existingTime > newTime) {
              return;
            }
          } else if (existing && existing.updatedAt != null && row.updatedAt == null) {
            return;
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

  async deleteDatabase(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
    }

    return new Promise((resolve, reject) => {
      const request = indexedDB.deleteDatabase(this.dbName);

      request.onsuccess = () => resolve();
      request.onerror = () => reject(new Error(`Failed to delete database: ${request.error}`));
      request.onblocked = () => reject(new Error('Database deletion blocked'));
    });
  }
}

export class IndexedDbService {
  private storage: IndexedDBStorage;
  private elmApp: ElmApp | null = null;

  constructor(storage: IndexedDBStorage) {
    this.storage = storage;
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.indexedDbOut) {
      elmApp.ports.indexedDbOut.subscribe((message) => {
        this.handleMessage(message as { type?: string; tableGroups?: TableGroup[] }).catch((error) => {
          console.error('[PyreClient] IndexedDB handler failed:', error);
        });
      });
    }
  }

  private async handleMessage(message: { type?: string; tableGroups?: TableGroup[] }): Promise<void> {
    if (message.type === 'requestInitialData') {
      await this.sendInitialData();
      return;
    }

    if (message.type === 'writeDelta') {
      await this.writeDelta(message.tableGroups || []);
    }
  }

  private async sendInitialData(): Promise<void> {
    if (!this.elmApp?.ports.receiveIndexedDbMessage) {
      return;
    }

    try {
      await this.storage.init();
      const tables = await this.storage.getAllTables();

      this.elmApp.ports.receiveIndexedDbMessage.send({
        type: 'initialData',
        data: { tables },
      });
    } catch (error) {
      console.error('[PyreClient] Failed to load initial data:', error);
      this.elmApp.ports.receiveIndexedDbMessage.send({
        type: 'initialData',
        data: { tables: {} },
      });
    }
  }

  private async writeDelta(tableGroups: TableGroup[]): Promise<void> {
    try {
      await this.storage.init();

      for (const tableGroup of tableGroups) {
        const tableName = tableGroup.table_name;
        if (!tableName) {
          continue;
        }

        const rows = tableGroup.rows.map((rowArray) => {
          const rowObj: Record<string, unknown> = {};
          tableGroup.headers.forEach((header, index) => {
            rowObj[header] = rowArray[index];
          });
          return rowObj;
        });

        await this.storage.putRows(tableName, rows);
      }
    } catch (error) {
      console.error('[PyreClient] Failed to write delta:', error);
    }
  }
}
