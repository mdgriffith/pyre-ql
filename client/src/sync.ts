/**
 * Sync/catchup logic for Pyre Client
 */

import type { SyncCursor, SyncPageResult, ClientConfig, SyncProgressCallback } from './types';
import { Storage } from './storage';

export class SyncManager {
  private storage: Storage;
  private config: ClientConfig;
  private sessionId: string | null = null;
  private syncing = false;
  private synced = false;
  private syncError: Error | null = null;
  private syncPromise: Promise<void> | null = null;
  private lastSyncedTables: Set<string> = new Set();

  constructor(storage: Storage, config: ClientConfig) {
    this.storage = storage;
    this.config = config;
  }

  setSessionId(sessionId: string) {
    this.sessionId = sessionId;
  }

  getStatus() {
    return {
      syncing: this.syncing,
      synced: this.synced,
      error: this.syncError || undefined,
    };
  }

  async sync(onProgress?: SyncProgressCallback): Promise<void> {
    if (!this.sessionId) {
      const error = new Error('Session ID not set. SSE must be connected first.');
      console.error('[PyreClient]', error.message);
      throw error;
    }

    // If already syncing, wait for the existing sync to complete
    if (this.syncing && this.syncPromise) {
      return this.syncPromise;
    }

    // Start new sync
    this.syncing = true;
    this.syncError = null;

    this.syncPromise = (async () => {
      try {
        await this.performSync(onProgress);
        this.synced = true;
      } catch (error) {
        this.syncError = error instanceof Error ? error : new Error(String(error));
        console.error('[PyreClient] Sync failed:', this.syncError);
        throw this.syncError;
      } finally {
        this.syncing = false;
        this.syncPromise = null;
      }
    })();

    return this.syncPromise;
  }

  private async performSync(onProgress?: SyncProgressCallback): Promise<void> {
    let cursor = await this.storage.getSyncCursor();

    // Validate cursor structure
    if (!cursor || typeof cursor !== 'object' || !cursor.tables) {
      console.warn('[PyreClient] Invalid sync cursor, resetting to empty');
      cursor = { tables: {} };
    }

    let hasMore = true;
    let tablesSynced = 0;
    const seenTables = new Set<string>();

    while (hasMore) {
      try {
        const result = await this.fetchSyncPage(cursor);

        // Process each table
        for (const [tableName, tableData] of Object.entries(result.tables)) {
          if (!seenTables.has(tableName)) {
            seenTables.add(tableName);
          }

          // Track this table as synced
          this.lastSyncedTables.add(tableName);

          // Update cursor for this table
          cursor.tables[tableName] = {
            last_seen_updated_at: tableData.last_seen_updated_at,
            permission_hash: tableData.permission_hash,
          };

          // Store rows
          try {
            await this.storage.putRows(tableName, tableData.rows);
          } catch (error) {
            console.error(`[PyreClient] Failed to store rows for table ${tableName}:`, error);
            throw error;
          }

          tablesSynced++;

          if (onProgress) {
            onProgress({
              table: tableName,
              tablesSynced,
              complete: false,
            });
          }
        }

        // Save cursor after each page
        try {
          await this.storage.saveSyncCursor(cursor);
        } catch (error) {
          console.error('[PyreClient] Failed to save sync cursor:', error);
          // Continue syncing even if cursor save fails
        }

        hasMore = result.has_more;
      } catch (error) {
        console.error('[PyreClient] Failed to fetch sync page:', error);
        throw error;
      }
    }

    if (onProgress) {
      onProgress({
        tablesSynced,
        complete: true,
      });
    }
  }

  getSyncedTables(): Set<string> {
    return new Set(this.lastSyncedTables);
  }

  private async fetchSyncPage(cursor: SyncCursor): Promise<SyncPageResult> {
    const maxRetries = this.config.retry.maxRetries;
    const initialDelay = this.config.retry.initialDelay;
    const maxDelay = this.config.retry.maxDelay;
    const backoffMultiplier = this.config.retry.backoffMultiplier;

    let lastError: Error | null = null;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        const syncCursorParam = encodeURIComponent(JSON.stringify(cursor));
        const url = `${this.config.baseUrl}/sync?sessionId=${encodeURIComponent(this.sessionId!)}&syncCursor=${syncCursorParam}`;

        const response = await fetch(url);

        if (!response.ok) {
          const errorText = await response.text().catch(() => '');
          const error = new Error(`Sync request failed: ${response.status} ${response.statusText}${errorText ? ` - ${errorText}` : ''}`);
          console.error('[PyreClient]', error.message);
          throw error;
        }

        const result: SyncPageResult = await response.json();
        return result;
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        if (attempt < maxRetries) {
          const delay = Math.min(
            initialDelay * Math.pow(backoffMultiplier, attempt),
            maxDelay
          );
          console.warn(`[PyreClient] Sync attempt ${attempt + 1} failed, retrying in ${delay}ms...`);
          await new Promise(resolve => setTimeout(resolve, delay));
        } else {
          console.error('[PyreClient] Sync failed after all retries');
        }
      }
    }

    throw lastError || new Error('Sync failed after retries');
  }

  /**
   * Apply a delta (array of affected rows) to the local database.
   * 
   * Delta format:
   * ```json
   * [
   *   {
   *     "table_name": "users",
   *     "row": { "id": 1, "name": "Alice" },
   *     "headers": ["id", "name"]
   *   }
   * ]
   * ```
   */
  async applyDelta(delta: Array<{ table_name: string; row: any; headers: string[] }>): Promise<void> {
    const rowsToUpdate: Record<string, any[]> = {};

    for (const affectedRow of delta) {
      const tableName = affectedRow.table_name;
      if (!tableName) {
        console.warn('[PyreClient] Delta row missing table_name');
        continue;
      }

      if (!rowsToUpdate[tableName]) {
        rowsToUpdate[tableName] = [];
      }

      // Convert row array to object if needed
      let row: any;
      if (Array.isArray(affectedRow.row)) {
        row = {};
        affectedRow.headers.forEach((header, i) => {
          row[header] = affectedRow.row[i];
        });
      } else {
        row = affectedRow.row;
      }

      rowsToUpdate[tableName].push(row);
    }

    // Apply updates with conflict resolution
    for (const [tableName, rows] of Object.entries(rowsToUpdate)) {
      try {
        await this.storage.putRows(tableName, rows);
      } catch (error) {
        console.error(`[PyreClient] Failed to apply delta for table ${tableName}:`, error);
        throw error;
      }
    }
  }
}
