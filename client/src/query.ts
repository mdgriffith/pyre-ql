/**
 * Query execution and live updates for Pyre Client
 */

import type { QueryShape, WhereClause, SortClause, Unsubscribe } from './types';
import { Storage } from './storage';
import { evaluateFilter, sortRows } from './filter';
import { SchemaManager } from './schema';

export interface QuerySubscription {
  data: any;
  unsubscribe: Unsubscribe;
}

interface QueryInfo {
  callbacks: Set<() => void>;
  tableDependencies: Set<string>;
}

export class QueryManager {
  private storage: Storage;
  private schemaManager: SchemaManager;
  private activeQueries = new Map<number, QueryInfo>();
  private nextQueryId = 0;

  constructor(storage: Storage, schemaManager: SchemaManager) {
    this.storage = storage;
    this.schemaManager = schemaManager;
  }

  query(shape: QueryShape, callback: (data: any) => void): Unsubscribe {
    const queryId = this.nextQueryId++;
    const tableDependencies = new Set<string>();

    // Extract query field dependencies from query shape (these are query field names, not table names)
    this.extractTableDependencies(shape, tableDependencies);

    // Initial execution
    const executeQuery = async () => {
      try {
        const result = await this.executeQueryShape(shape);
        if (result === null || result === undefined) {
          console.warn('[QueryManager] Query returned null/undefined result');
          callback({}); // Return empty object instead of null
          return;
        }
        callback(result);
      } catch (error) {
        console.error('[PyreClient] Query execution failed:', error);
        callback({}); // Return empty object on error instead of null
      }
    };

    executeQuery();

    // Store query info for live updates
    const queryInfo: QueryInfo = {
      callbacks: new Set([executeQuery]),
      tableDependencies,
    };
    this.activeQueries.set(queryId, queryInfo);

    // Return unsubscribe function
    return () => {
      this.activeQueries.delete(queryId);
    };
  }

  private extractTableDependencies(shape: QueryShape, dependencies: Set<string>) {
    // Extract query field names (not table names) from query shape
    for (const [queryFieldName, fieldSpec] of Object.entries(shape)) {
      dependencies.add(queryFieldName);
      this.extractDependenciesFromFieldSpec(queryFieldName, fieldSpec, dependencies);
    }
  }

  private extractDependenciesFromFieldSpec(queryFieldName: string, fieldSpec: any, dependencies: Set<string>) {
    // Map query field name to table name for relationship lookup
    const tableName = this.schemaManager.getTableNameFromQueryField(queryFieldName);
    if (!tableName) {
      return; // Can't extract dependencies without table name
    }

    for (const [field, selection] of Object.entries(fieldSpec)) {
      if (field.startsWith('@')) {
        continue; // Skip special directives
      }
      if (selection === true) {
        continue; // Simple field selection
      }
      if (typeof selection === 'object' && selection !== null) {
        // Check if this is a relationship
        const relInfo = this.schemaManager.getRelationshipInfo(tableName, field);
        if (relInfo.relatedTable) {
          // Map related table name back to query field name
          // Find the query field name that maps to this table
          const relatedQueryFieldName = this.findQueryFieldNameForTable(relInfo.relatedTable);
          if (relatedQueryFieldName) {
            dependencies.add(relatedQueryFieldName);
            // Recursively extract dependencies from nested selections
            this.extractDependenciesFromFieldSpec(relatedQueryFieldName, selection, dependencies);
          } else {
            // Fallback: if we can't find the query field name, use the table name
            // This shouldn't happen in normal operation, but provides a fallback
            dependencies.add(relInfo.relatedTable);
            this.extractDependenciesFromFieldSpec(relInfo.relatedTable, selection, dependencies);
          }
        }
      }
    }
  }

  private findQueryFieldNameForTable(tableName: string): string | null {
    const schemaMetadata = this.schemaManager.getSchemaMetadata();
    if (!schemaMetadata) {
      return null;
    }
    // Reverse lookup: find query field name that maps to this table name
    for (const [queryFieldName, mappedTableName] of Object.entries(schemaMetadata.queryFieldToTable)) {
      if (mappedTableName === tableName) {
        return queryFieldName;
      }
    }
    return null;
  }

  notifyQueries(tableNames: string[]) {
    const affectedTableSet = new Set(tableNames);

    // Only notify queries that depend on the affected tables
    for (const [queryId, queryInfo] of this.activeQueries.entries()) {
      const dependencies = Array.from(queryInfo.tableDependencies);
      // Check if any dependency matches affected tables
      // Dependencies are query field names, so we need to map them to table names
      const hasDependency = dependencies.some(dep => {
        const tableName = this.schemaManager.getTableNameFromQueryField(dep);
        return tableName ? affectedTableSet.has(tableName) : false;
      });

      if (hasDependency) {
        for (const callback of queryInfo.callbacks) {
          try {
            callback();
          } catch (error) {
            console.error('[PyreClient] Query callback failed:', error);
          }
        }
      }
    }
  }

  private async executeQueryShape(shape: QueryShape): Promise<any> {
    const result: any = {};
    console.log('[QueryManager] executeQueryShape - shape:', JSON.stringify(shape, null, 2));

    for (const [queryFieldName, fieldSpec] of Object.entries(shape)) {
      // Map query field name to table name using schema metadata
      const tableName = this.schemaManager.getTableNameFromQueryField(queryFieldName);
      console.log(`[QueryManager] Processing query field "${queryFieldName}" -> table "${tableName}"`);
      if (!tableName) {
        console.error(`[QueryManager] Could not find table name for query field: ${queryFieldName}`);
        // Still add an empty array to maintain structure
        result[queryFieldName] = [];
        continue;
      }

      try {
        // Execute using table name, but store result under query field name
        const data = await this.executeFieldSpec(tableName, fieldSpec);
        console.log(`[QueryManager] Query field "${queryFieldName}" (table "${tableName}") returned ${Array.isArray(data) ? data.length : 'non-array'} results`);
        // Ensure we always have an array (even if empty)
        result[queryFieldName] = Array.isArray(data) ? data : (data ? [data] : []);
      } catch (error) {
        console.error(`[QueryManager] Error executing query for field ${queryFieldName}:`, error);
        result[queryFieldName] = [];
      }
    }

    console.log('[QueryManager] executeQueryShape - final result:', JSON.stringify(result, null, 2));
    return result;
  }

  private async executeFieldSpec(tableName: string, fieldSpec: any): Promise<any> {
    console.log(`[QueryManager] executeFieldSpec - table: "${tableName}", fieldSpec:`, JSON.stringify(fieldSpec, null, 2));
    
    // Extract special directives
    const where = fieldSpec['@where'] as WhereClause | undefined;
    const sort = fieldSpec['@sort'] as SortClause | SortClause[] | undefined;
    const limit = fieldSpec['@limit'] as number | undefined;

    // Get all rows for this table
    let rows = await this.storage.getAllRows(tableName);
    console.log(`[QueryManager] getAllRows("${tableName}") returned ${rows.length} rows:`, rows.map(r => ({ id: r.id, ...(r.tableName ? { tableName: r.tableName } : {}) })));

    // Apply filter
    if (where) {
      const beforeCount = rows.length;
      rows = rows.filter(row => evaluateFilter(row, where));
      console.log(`[QueryManager] After applying @where filter: ${beforeCount} -> ${rows.length} rows`);
    }

    // Apply sorting
    if (sort) {
      const sortArray = Array.isArray(sort) ? sort : [sort];
      rows = sortRows(rows, sortArray.map(s => ({
        field: s.field,
        direction: s.direction.toLowerCase() === 'desc' ? 'desc' : 'asc'
      })));
      console.log(`[QueryManager] Applied sorting, ${rows.length} rows`);
    }

    // Apply limit
    if (limit !== undefined) {
      const beforeLimit = rows.length;
      rows = rows.slice(0, limit);
      console.log(`[QueryManager] Applied limit ${limit}: ${beforeLimit} -> ${rows.length} rows`);
    }

    // Extract field selections (everything except special directives)
    const fieldSelections: Record<string, any> = {};
    for (const [key, value] of Object.entries(fieldSpec)) {
      if (key.startsWith('@')) {
        continue; // Skip special directives
      }
      fieldSelections[key] = value;
    }

    // If no field selections, return all fields
    if (Object.keys(fieldSelections).length === 0) {
      return rows.map(row => this.removeTableName(row));
    }

    // Project fields
    const projectedRows = await Promise.all(rows.map(row => {
      return this.projectFields(row, fieldSelections, tableName);
    }));

    return projectedRows;
  }

  private async projectFields(row: any, fieldSelections: Record<string, any>, currentTableName: string): Promise<any> {
    const projected: any = {};
    console.log(`[QueryManager] projectFields - table: "${currentTableName}", row id: ${row.id}, selections:`, Object.keys(fieldSelections));

    for (const [field, selection] of Object.entries(fieldSelections)) {
      if (selection === true) {
        // Simple field selection
        projected[field] = row[field];
        console.log(`[QueryManager] projectFields - simple field "${field}":`, row[field]);
      } else if (typeof selection === 'object' && selection !== null) {
        // Nested selection or relationship
        if (this.isRelationshipField(currentTableName, field)) {
          // This is a relationship field - resolve it
          console.log(`[QueryManager] projectFields - resolving relationship field "${field}"`);
          projected[field] = await this.resolveRelationship(field, row, selection, currentTableName);
          console.log(`[QueryManager] projectFields - relationship "${field}" resolved to:`, Array.isArray(projected[field]) ? `${projected[field].length} items` : projected[field]);
        } else {
          // Nested object selection
          projected[field] = await this.projectFields(row[field] || {}, selection, currentTableName);
        }
      }
    }

    return projected;
  }

  private isRelationshipField(tableName: string, fieldName: string): boolean {
    const relInfo = this.schemaManager.getRelationshipInfo(tableName, fieldName);
    return relInfo.type !== null;
  }

  private async resolveRelationship(fieldName: string, row: any, selection: any, currentTableName: string): Promise<any> {
    const relInfo = this.schemaManager.getRelationshipInfo(currentTableName, fieldName);
    console.log(`[QueryManager] resolveRelationship - field: "${fieldName}", table: "${currentTableName}", relInfo:`, relInfo);
    console.log(`[QueryManager] resolveRelationship - row:`, JSON.stringify(row, null, 2));

    if (!relInfo.type || !relInfo.relatedTable || !relInfo.fromColumn) {
      console.warn(`[QueryManager] resolveRelationship - missing relInfo: type=${relInfo.type}, relatedTable=${relInfo.relatedTable}, fromColumn=${relInfo.fromColumn}`);
      return null;
    }

    if (relInfo.type === 'one-to-many') {
      // One-to-many: get all rows from related table where foreignKeyField (FK in foreign table) matches this row's fromColumn (PK in current table)
      if (!relInfo.foreignKeyField) {
        console.warn(`[QueryManager] resolveRelationship - one-to-many missing foreignKeyField`);
        return [];
      }

      const lookupValue = row[relInfo.fromColumn];
      console.log(`[QueryManager] resolveRelationship - one-to-many lookup: ${relInfo.relatedTable}.${relInfo.foreignKeyField} = ${lookupValue} (from ${currentTableName}.${relInfo.fromColumn})`);
      
      const matchingRows = await this.storage.getRowsByForeignKey(
        relInfo.relatedTable,
        relInfo.foreignKeyField,
        lookupValue
      );

      console.log(`[QueryManager] resolveRelationship - one-to-many found ${matchingRows.length} matching rows`);

      // Apply selection to each row
      if (selection === true) {
        return matchingRows.map(r => this.removeTableName(r));
      } else {
        // Need to project fields for each matching row
        const projected = await Promise.all(matchingRows.map(r =>
          this.projectFields(r, selection, relInfo.relatedTable!)
        ));
        return projected;
      }
    } else {
      // Many-to-one or one-to-one: get single related row
      // foreignKeyField is the PK in the foreign table, fromColumn is the FK in the current table
      if (!relInfo.foreignKeyField) {
        console.warn(`[QueryManager] resolveRelationship - many-to-one/one-to-one missing foreignKeyField`);
        return null;
      }

      const foreignKeyId = row[relInfo.fromColumn];
      console.log(`[QueryManager] resolveRelationship - many-to-one/one-to-one lookup: ${relInfo.relatedTable}.${relInfo.foreignKeyField} = ${foreignKeyId} (from ${currentTableName}.${relInfo.fromColumn})`);
      
      if (foreignKeyId === null || foreignKeyId === undefined) {
        console.log(`[QueryManager] resolveRelationship - foreignKeyId is null/undefined, returning null`);
        return null;
      }

      const relatedRow = await this.storage.getRow(relInfo.relatedTable, foreignKeyId);
      console.log(`[QueryManager] resolveRelationship - many-to-one/one-to-one found row:`, relatedRow ? 'yes' : 'no');

      if (!relatedRow) {
        return null;
      }

      // If selection is just true, return the whole row
      if (selection === true) {
        return this.removeTableName(relatedRow);
      }

      // Otherwise, project the selected fields
      return this.projectFields(relatedRow, selection, relInfo.relatedTable);
    }
  }

  private removeTableName(row: any): any {
    // Remove the tableName field that we add for IndexedDB storage
    const { tableName, ...rest } = row;
    return rest;
  }
}
