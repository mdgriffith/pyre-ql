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
        const cursor = request.result;
        if (cursor && typeof cursor === 'object' && cursor.tables) {
          resolve(cursor);
        } else {
          // No cursor found or invalid format, return empty cursor
          resolve({ tables: {} });
        }
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
      
      // First, let's check what's actually in the database
      const allRequest = store.getAll();
      allRequest.onsuccess = () => {
        const allRows = allRequest.result || [];
        console.log(`[Storage] getAllRows("${tableName}") - Total rows in database: ${allRows.length}`);
        
        // Group by tableName to see what tables exist
        const byTable: Record<string, any[]> = {};
        allRows.forEach(row => {
          const rowTableName = row.tableName || 'NO_TABLE_NAME';
          if (!byTable[rowTableName]) {
            byTable[rowTableName] = [];
          }
          byTable[rowTableName].push(row);
        });
        console.log(`[Storage] Rows by table:`, Object.keys(byTable).map(t => `${t}: ${byTable[t].length}`).join(', '));
        
        if (byTable[tableName]) {
          console.log(`[Storage] Found ${byTable[tableName].length} rows for table "${tableName}"`);
          if (byTable[tableName].length > 0) {
            console.log(`[Storage] Sample row for "${tableName}":`, JSON.stringify(byTable[tableName][0], null, 2));
          }
        } else {
          console.warn(`[Storage] No rows found for table "${tableName}" in direct scan. Available tables:`, Object.keys(byTable));
        }
      };
      
      const index = store.index('byTable');
      const range = IDBKeyRange.only(tableName);
      const request = index.getAll(range);

      request.onsuccess = () => {
        const result = request.result || [];
        console.log(`[Storage] getAllRows("${tableName}") - index query returned ${result.length} rows`);
        if (result.length > 0) {
          console.log(`[Storage] Sample row from index:`, JSON.stringify(result[0], null, 2));
        } else {
          console.warn(`[Storage] Index query returned 0 rows for "${tableName}" - checking if index is working correctly`);
        }
        resolve(result);
      };

      request.onerror = () => {
        console.error(`[Storage] Failed to read rows for table "${tableName}":`, request.error);
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

      // Wait for transaction to complete, not just individual requests
      tx.oncomplete = () => {
        if (error) {
          reject(error);
        } else {
          resolve();
        }
      };

      // Handle transaction errors
      tx.onerror = () => {
        console.error(`[PyreClient] Transaction error for putRows('${tableName}'):`, tx.error);
        reject(new Error(`Transaction failed: ${tx.error}`));
      };

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
              return;
            }
          } else if (existing && existing.updatedAt != null && row.updatedAt == null) {
            // Existing has updatedAt but new row doesn't - keep existing
            writesCompleted++;
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
          };

          request.onerror = () => {
            error = new Error(`Failed to write row: ${request.error}`);
            writesCompleted++;
          };
        });
        
        // If no writes were started (all skipped), transaction will complete naturally
        if (writesStarted === 0) {
          // All rows were skipped, transaction will complete
        }
      }
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
      const range = IDBKeyRange.only(tableName);
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
    console.log(`[Storage] getRowsByForeignKey - table: "${tableName}", field: "${foreignKeyField}", value: ${foreignKeyValue} (type: ${typeof foreignKeyValue})`);
    const allRows = await this.getAllRows(tableName);
    console.log(`[Storage] getRowsByForeignKey - fetched ${allRows.length} total rows, filtering by ${foreignKeyField} = ${foreignKeyValue}`);
    
    if (allRows.length > 0) {
      console.log(`[Storage] getRowsByForeignKey - sample row fields:`, Object.keys(allRows[0]));
      console.log(`[Storage] getRowsByForeignKey - sample row ${foreignKeyField} value:`, allRows[0][foreignKeyField], `(type: ${typeof allRows[0][foreignKeyField]})`);
    }
    
    if (allRows.length > 1000) {
      console.warn(
        `[PyreClient] getRowsByForeignKey: Filtering ${allRows.length} rows for ${tableName}.${foreignKeyField}. ` +
        `Consider creating an index for better performance.`
      );
    }
    const filtered = allRows.filter(row => {
      const fkValue = row[foreignKeyField];
      const matches = fkValue === foreignKeyValue || String(fkValue) === String(foreignKeyValue);
      if (!matches && allRows.length <= 10) {
        console.log(`[Storage] getRowsByForeignKey - row ${row.id} doesn't match: ${fkValue} !== ${foreignKeyValue}`);
      }
      return matches;
    });
    console.log(`[Storage] getRowsByForeignKey - filtered to ${filtered.length} matching rows`);
    return filtered;
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

  async deleteDatabase(): Promise<void> {
    // Close existing connection if open (this will abort any pending transactions)
    if (this.db) {
      this.db.close();
      this.db = null;
    }
    this.initPromise = null;

    // Wait a bit for the connection to fully close
    await new Promise(resolve => setTimeout(resolve, 200));

    return new Promise((resolve, reject) => {
      const deleteRequest = indexedDB.deleteDatabase(this.dbName);

      deleteRequest.onsuccess = () => {
        // Small delay to ensure deletion is fully processed
        setTimeout(() => {
          resolve();
        }, 100);
      };

      deleteRequest.onerror = () => {
        console.error(`[PyreClient] Failed to delete database: ${deleteRequest.error}`, deleteRequest.error);
        reject(new Error(`Failed to delete database: ${deleteRequest.error}`));
      };

      deleteRequest.onblocked = () => {
        // Database is blocked (probably open in another tab)
        console.warn(`[PyreClient] Database deletion blocked for: ${this.dbName} - this usually means it's open in another tab`);
        // Don't reject, just warn - the deletion will proceed when unblocked
        setTimeout(() => {
          resolve();
        }, 1000);
      };
    });
  }
}
