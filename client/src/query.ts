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

    // Extract table dependencies from query shape
    this.extractTableDependencies(shape, tableDependencies);

    // Initial execution
    const executeQuery = async () => {
      try {
        const result = await this.executeQueryShape(shape);
        callback(result);
      } catch (error) {
        console.error('[PyreClient] Query execution failed:', error);
        callback(null); // Return null on error
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
    for (const [tableName, fieldSpec] of Object.entries(shape)) {
      dependencies.add(tableName);
      this.extractDependenciesFromFieldSpec(tableName, fieldSpec, dependencies);
    }
  }

  private extractDependenciesFromFieldSpec(tableName: string, fieldSpec: any, dependencies: Set<string>) {
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
          dependencies.add(relInfo.relatedTable);
          // Recursively extract dependencies from nested selections
          this.extractDependenciesFromFieldSpec(relInfo.relatedTable, selection, dependencies);
        }
      }
    }
  }

  notifyQueries(tableNames: string[]) {
    const affectedTableSet = new Set(tableNames);
    
    // Only notify queries that depend on the affected tables
    for (const [queryId, queryInfo] of this.activeQueries.entries()) {
      const dependencies = Array.from(queryInfo.tableDependencies);
      const hasDependency = dependencies.some(dep => affectedTableSet.has(dep));
      
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

    for (const [tableName, fieldSpec] of Object.entries(shape)) {
      result[tableName] = await this.executeFieldSpec(tableName, fieldSpec);
    }

    return result;
  }

  private async executeFieldSpec(tableName: string, fieldSpec: any): Promise<any> {
    // Extract special directives
    const where = fieldSpec['@where'] as WhereClause | undefined;
    const sort = fieldSpec['@sort'] as SortClause | SortClause[] | undefined;
    const limit = fieldSpec['@limit'] as number | undefined;

    // Get all rows for this table
    let rows = await this.storage.getAllRows(tableName);

    // Apply filter
    if (where) {
      rows = rows.filter(row => evaluateFilter(row, where));
    }

    // Apply sorting
    if (sort) {
      const sortArray = Array.isArray(sort) ? sort : [sort];
      rows = sortRows(rows, sortArray.map(s => ({
        field: s.field,
        direction: s.direction.toLowerCase() === 'desc' ? 'desc' : 'asc'
      })));
    }

    // Apply limit
    if (limit !== undefined) {
      rows = rows.slice(0, limit);
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

    for (const [field, selection] of Object.entries(fieldSelections)) {
      if (selection === true) {
        // Simple field selection
        projected[field] = row[field];
      } else if (typeof selection === 'object' && selection !== null) {
        // Nested selection or relationship
        if (this.isRelationshipField(currentTableName, field)) {
          // This is a relationship field - resolve it
          projected[field] = await this.resolveRelationship(field, row, selection, currentTableName);
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
    
    if (!relInfo.type || !relInfo.relatedTable) {
      return null;
    }

    if (relInfo.type === 'one-to-many') {
      // One-to-many: get all rows from related table where foreignKeyId matches this row's id
      if (!relInfo.foreignKeyField) {
        return [];
      }

      const matchingRows = await this.storage.getRowsByForeignKey(
        relInfo.relatedTable,
        relInfo.foreignKeyField,
        row.id
      );

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
      // Many-to-one: get single related row
      if (!relInfo.foreignKeyField) {
        return null;
      }

      const foreignKeyId = row[relInfo.foreignKeyField];
      if (foreignKeyId === null || foreignKeyId === undefined) {
        return null;
      }

      const relatedRow = await this.storage.getRow(relInfo.relatedTable, foreignKeyId);

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
