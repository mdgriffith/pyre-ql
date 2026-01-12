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
          // Index for foreign keys - we'll create these dynamically based on schema
        }

        // Create syncCursor object store if it doesn't exist
        if (!db.objectStoreNames.contains('syncCursor')) {
          db.createObjectStore('syncCursor');
        }

        // Create schema object store if it doesn't exist
        if (!db.objectStoreNames.contains('schema')) {
          db.createObjectStore('schema');
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
      
      let error: Error | null = null;
      const existingRows: (any | null)[] = new Array(rows.length);
      let readsCompleted = 0;
      let writesCompleted = 0;
      let writesStarted = 0;

      // First, read all existing rows
      rows.forEach((row, index) => {
        const request = store.get([tableName, row.id]);
        request.onsuccess = () => {
          existingRows[index] = request.result || null;
          readsCompleted++;
          
          // Once all reads are done, process writes
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

      function processWrites() {
        rows.forEach((row, index) => {
          const existing = existingRows[index];
          
          // Conflict resolution: newer updatedAt wins (assumes all tables have updatedAt)
          if (existing && existing.updatedAt != null && row.updatedAt != null) {
            const existingTime = typeof existing.updatedAt === 'number' 
              ? existing.updatedAt 
              : new Date(existing.updatedAt).getTime() / 1000;
            const newTime = typeof row.updatedAt === 'number'
              ? row.updatedAt
              : new Date(row.updatedAt).getTime() / 1000;
            
            if (existingTime > newTime) {
              // Existing is newer, skip this row
              writesCompleted++;
              checkComplete();
              return;
            }
          } else if (existing && existing.updatedAt != null && row.updatedAt == null) {
            // Existing has updatedAt but new row doesn't - keep existing
            writesCompleted++;
            checkComplete();
            return;
          } else if (existing && existing.updatedAt == null && row.updatedAt != null) {
            // New row has updatedAt but existing doesn't - use new row (continue below)
          } else if (existing && existing.updatedAt == null && row.updatedAt == null) {
            // Neither has updatedAt - use new row (continue below)
          }

          // Add tableName to row for composite key
          const rowWithTable = { ...row, tableName };
          const request = store.put(rowWithTable);
          writesStarted++;

          request.onsuccess = () => {
            writesCompleted++;
            checkComplete();
          };

          request.onerror = () => {
            error = new Error(`Failed to write row: ${request.error}`);
            writesCompleted++;
            checkComplete();
          };
        });
        
        // If no writes were started (all skipped), check completion
        if (writesStarted === 0) {
          checkComplete();
        }
      }

      function checkComplete() {
        if (writesCompleted === rows.length) {
          if (error) {
            reject(error);
          } else {
            resolve();
          }
        }
      }

      // Handle transaction errors
      tx.onerror = () => {
        reject(new Error(`Transaction failed: ${tx.error}`));
      };
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

  async saveSchema(schemaSource: string): Promise<void> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['schema'], 'readwrite');
      const store = tx.objectStore('schema');
      const request = store.put(schemaSource, 'schema');

      request.onsuccess = () => {
        resolve();
      };

      request.onerror = () => {
        reject(new Error(`Failed to save schema: ${request.error}`));
      };
    });
  }

  async getSchema(): Promise<string | null> {
    const db = await this.getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(['schema'], 'readonly');
      const store = tx.objectStore('schema');
      const request = store.get('schema');

      request.onsuccess = () => {
        const result = request.result;
        resolve(typeof result === 'string' ? result : null);
      };

      request.onerror = () => {
        reject(new Error(`Failed to read schema: ${request.error}`));
      };
    });
  }

  async createForeignKeyIndex(tableName: string, foreignKeyField: string): Promise<void> {
    const db = await this.getDB();
    // Note: IndexedDB doesn't support creating indexes after object store creation in the same transaction
    // This would need to be done during upgrade, but we can't dynamically add indexes easily
    // For now, we'll rely on the byTable index and filter in memory
    // In a production system, you'd want to create all indexes during the initial schema setup
  }

  async getRowsByForeignKey(
    tableName: string,
    foreignKeyField: string,
    foreignKeyValue: any
  ): Promise<any[]> {
    // Since we can't easily create dynamic indexes, we'll fetch all rows and filter
    // This is acceptable for now, but could be optimized with proper index setup
    // WARNING: This is inefficient for large tables - consider creating indexes during schema setup
    const allRows = await this.getAllRows(tableName);
    if (allRows.length > 1000) {
      console.warn(
        `[PyreClient] getRowsByForeignKey: Filtering ${allRows.length} rows for ${tableName}.${foreignKeyField}. ` +
        `Consider creating an index for better performance.`
      );
    }
    return allRows.filter(row => {
      const fkValue = row[foreignKeyField];
      return fkValue === foreignKeyValue || String(fkValue) === String(foreignKeyValue);
    });
  }

  async checkStorageQuota(): Promise<{ usage: number; quota: number; percentage: number }> {
    if ('storage' in navigator && 'estimate' in navigator.storage) {
      try {
        const estimate = await navigator.storage.estimate();
        const usage = estimate.usage || 0;
        const quota = estimate.quota || 0;
        const percentage = quota > 0 ? (usage / quota) * 100 : 0;
        return { usage, quota, percentage };
      } catch (error) {
        console.warn('[PyreClient] Failed to check storage quota:', error);
        return { usage: 0, quota: 0, percentage: 0 };
      }
    }
    return { usage: 0, quota: 0, percentage: 0 };
  }
}
