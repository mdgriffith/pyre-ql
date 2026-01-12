/**
 * Sync/catchup logic for Pyre Client
 */

import type { SyncCursor, SyncPageResult, ClientConfig, SyncProgressCallback } from './types';
import { Storage } from './storage';

export class SyncManager {
  private storage: Storage;
  private config: Required<ClientConfig>;
  private sessionId: string | null = null;
  private syncing = false;
  private synced = false;
  private syncError: Error | null = null;

  constructor(storage: Storage, config: Required<ClientConfig>) {
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
      throw new Error('Session ID not set. WebSocket must be connected first.');
    }

    if (this.syncing) {
      // Already syncing, wait for it
      return new Promise((resolve, reject) => {
        const checkInterval = setInterval(() => {
          if (!this.syncing) {
            clearInterval(checkInterval);
            if (this.syncError) {
              reject(this.syncError);
            } else {
              resolve();
            }
          }
        }, 100);
      });
    }

    this.syncing = true;
    this.syncError = null;

    try {
      await this.performSync(onProgress);
      this.synced = true;
    } catch (error) {
      this.syncError = error instanceof Error ? error : new Error(String(error));
      throw this.syncError;
    } finally {
      this.syncing = false;
    }
  }

  private async performSync(onProgress?: SyncProgressCallback): Promise<void> {
    let cursor = await this.storage.getSyncCursor();
    let hasMore = true;
    let tablesSynced = 0;
    const seenTables = new Set<string>();

    while (hasMore) {
      const result = await this.fetchSyncPage(cursor);
      
      // Process each table
      for (const [tableName, tableData] of Object.entries(result.tables)) {
        if (!seenTables.has(tableName)) {
          seenTables.add(tableName);
        }

        // Update cursor for this table
        cursor.tables[tableName] = {
          last_seen_updated_at: tableData.last_seen_updated_at,
          permission_hash: tableData.permission_hash,
        };

        // Store rows
        await this.storage.putRows(tableName, tableData.rows);

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
      await this.storage.saveSyncCursor(cursor);

      hasMore = result.has_more;
    }

    if (onProgress) {
      onProgress({
        tablesSynced,
        complete: true,
      });
    }
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
          throw new Error(`Sync request failed: ${response.status} ${response.statusText}`);
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
          await new Promise(resolve => setTimeout(resolve, delay));
        }
      }
    }

    throw lastError || new Error('Sync failed after retries');
  }

  async applyDelta(delta: {
    all_affected_rows: Array<{ table_name: string; row: any; headers: string[] }>;
    affected_row_indices: number[];
  }): Promise<void> {
    const rowsToUpdate: Record<string, any[]> = {};

    for (const index of delta.affected_row_indices) {
      const affectedRow = delta.all_affected_rows[index];
      if (!affectedRow) {
        continue;
      }

      const tableName = affectedRow.table_name;
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
      await this.storage.putRows(tableName, rows);
    }
  }
}
