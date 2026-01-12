/**
 * IndexedDB storage layer for Pyre Client
 */

import type { SyncCursor } from './types';

const DB_VERSION = 1;
const CURSOR_KEY = 'cursor';

export class Storage {
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

        // Create tables object store if it doesn't exist
        if (!db.objectStoreNames.contains('tables')) {
          const tablesStore = db.createObjectStore('tables', { keyPath: ['tableName', 'id'] });
          tablesStore.createIndex('byTable', 'tableName', { unique: false });
          tablesStore.createIndex('byUpdatedAt', 'updatedAt', { unique: false });
        }

        // Create syncCursor object store if it doesn't exist
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
    return this.db!;
  }

  async getSyncCursor(): Promise<SyncCursor> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['syncCursor'], 'readonly');
      const store = tx.objectStore('syncCursor');
      const request = store.get(CURSOR_KEY);

      request.onsuccess = () => {
        const cursor = request.result || { tables: {} };
        resolve(cursor);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read sync cursor: ${request.error}`));
      };
    });
  }

  async saveSyncCursor(cursor: SyncCursor): Promise<void> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['syncCursor'], 'readwrite');
      const store = tx.objectStore('syncCursor');
      const request = store.put(cursor, CURSOR_KEY);

      request.onsuccess = () => {
        resolve();
      };

      request.onerror = () => {
        reject(new Error(`Failed to save sync cursor: ${request.error}`));
      };
    });
  }

  async getAllRows(tableName: string): Promise<any[]> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const index = store.index('byTable');
      const range = IDBKeyRange.bound([tableName, ''], [tableName, '\uffff']);
      const request = index.getAll(range);

      request.onsuccess = () => {
        resolve(request.result || []);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read rows: ${request.error}`));
      };
    });
  }

  async getRow(tableName: string, id: any): Promise<any | null> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readonly');
      const store = tx.objectStore('tables');
      const request = store.get([tableName, id]);

      request.onsuccess = () => {
        resolve(request.result || null);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read row: ${request.error}`));
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

      // Get existing rows to check updatedAt for conflict resolution
      const existingPromises = rows.map(row => {
        return new Promise<any | null>((resolve) => {
          const request = store.get([tableName, row.id]);
          request.onsuccess = () => resolve(request.result || null);
          request.onerror = () => resolve(null);
        });
      });

      Promise.all(existingPromises).then(existingRows => {
        let error: Error | null = null;
        let completed = 0;

        rows.forEach((row, index) => {
          const existing = existingRows[index];
          
          // Conflict resolution: newer updatedAt wins
          if (existing && existing.updatedAt && row.updatedAt) {
            const existingTime = typeof existing.updatedAt === 'number' 
              ? existing.updatedAt 
              : new Date(existing.updatedAt).getTime() / 1000;
            const newTime = typeof row.updatedAt === 'number'
              ? row.updatedAt
              : new Date(row.updatedAt).getTime() / 1000;
            
            if (existingTime > newTime) {
              // Existing is newer, skip this row
              completed++;
              if (completed === rows.length) {
                if (error) reject(error);
                else resolve();
              }
              return;
            }
          }

          // Add tableName to row for composite key
          const rowWithTable = { ...row, tableName };
          const request = store.put(rowWithTable);

          request.onsuccess = () => {
            completed++;
            if (completed === rows.length) {
              if (error) reject(error);
              else resolve();
            }
          };

          request.onerror = () => {
            error = new Error(`Failed to write row: ${request.error}`);
            completed++;
            if (completed === rows.length) {
              reject(error);
            }
          };
        });
      });
    });
  }

  async deleteRow(tableName: string, id: any): Promise<void> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readwrite');
      const store = tx.objectStore('tables');
      const request = store.delete([tableName, id]);

      request.onsuccess = () => {
        resolve();
      };

      request.onerror = () => {
        reject(new Error(`Failed to delete row: ${request.error}`));
      };
    });
  }

  async clearTable(tableName: string): Promise<void> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['tables'], 'readwrite');
      const store = tx.objectStore('tables');
      const index = store.index('byTable');
      const range = IDBKeyRange.bound([tableName, ''], [tableName, '\uffff']);
      const request = index.openCursor(range);

      request.onsuccess = (event) => {
        const cursor = (event.target as IDBRequest).result;
        if (cursor) {
          cursor.delete();
          cursor.continue();
        } else {
          resolve();
        }
      };

      request.onerror = () => {
        reject(new Error(`Failed to clear table: ${request.error}`));
      };
    });
  }
}
