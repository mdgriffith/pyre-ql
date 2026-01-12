/**
 * Query execution and live updates for Pyre Client
 */

import type { QueryShape, WhereClause, SortClause, Unsubscribe } from './types';
import { Storage } from './storage';
import { evaluateFilter, sortRows } from './filter';

export interface QuerySubscription {
  data: any;
  unsubscribe: Unsubscribe;
}

export class QueryManager {
  private storage: Storage;
  private activeQueries = new Map<number, Set<() => void>>();
  private nextQueryId = 0;

  constructor(storage: Storage) {
    this.storage = storage;
  }

  query(shape: QueryShape, callback: (data: any) => void): Unsubscribe {
    const queryId = this.nextQueryId++;
    const updateCallbacks = new Set<() => void>();

    // Initial execution
    const executeQuery = async () => {
      const result = await this.executeQueryShape(shape);
      callback(result);
    };

    executeQuery();
    updateCallbacks.add(executeQuery);

    // Store query for live updates
    this.activeQueries.set(queryId, updateCallbacks);

    // Return unsubscribe function
    return () => {
      this.activeQueries.delete(queryId);
    };
  }

  notifyQueries(tableNames: string[]) {
    // Notify all queries that might be affected by changes to these tables
    for (const callbacks of this.activeQueries.values()) {
      for (const callback of callbacks) {
        callback();
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
        if (this.isRelationshipField(field, row)) {
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

  private isRelationshipField(fieldName: string, row: any): boolean {
    // Check if this field name matches a foreign key pattern
    // Common patterns: fieldId, field_id, or fieldNameId
    // Also check for reverse relationships (e.g., "users" field with "authorUserId" in row)
    
    // Direct foreign key check
    const idField = `${fieldName}Id`;
    const idFieldSnake = `${fieldName}_id`;
    if (idField in row || idFieldSnake in row) {
      return true;
    }
    
    // Check for reverse relationship patterns
    // e.g., "users" field when row has "authorUserId" -> look for "user" table
    const possibleIdFields = Object.keys(row).filter(key => 
      key.toLowerCase().includes(fieldName.toLowerCase()) || 
      fieldName.toLowerCase().includes(key.toLowerCase().replace(/id$/, '').replace(/_id$/, ''))
    );
    
    return possibleIdFields.length > 0;
  }

  private async resolveRelationship(fieldName: string, row: any, selection: any, currentTableName: string): Promise<any> {
    // Try to find the foreign key field
    // Common patterns: fieldNameId, fieldName_id, or reverse (e.g., "users" -> "authorUserId")
    
    let foreignKeyId: any = null;
    let relatedTableName: string | null = null;
    let isOneToMany = false;
    
    // Try direct patterns first (many-to-one: row has foreignKeyId pointing to related table)
    const idField = `${fieldName}Id`;
    const idFieldSnake = `${fieldName}_id`;
    
    if (idField in row) {
      foreignKeyId = row[idField];
      relatedTableName = this.inferTableNameFromField(fieldName);
    } else if (idFieldSnake in row) {
      foreignKeyId = row[idFieldSnake];
      relatedTableName = this.inferTableNameFromField(fieldName);
    } else {
      // Try reverse relationship - one-to-many (related table has foreignKeyId pointing to this row)
      // e.g., if fieldName is "posts" and this is a User row, then Post.authorUserId -> User.id
      
      // Check if this might be a one-to-many relationship
      // The related table would have a foreign key pointing to this row's id
      const possibleFkFields = this.inferForeignKeyFields(fieldName, currentTableName);
      
      if (possibleFkFields.length > 0) {
        // This is likely a one-to-many relationship
        isOneToMany = true;
        relatedTableName = this.inferTableNameFromField(fieldName);
      } else {
        // Try to find a foreign key in this row that matches the field name
        for (const [key, value] of Object.entries(row)) {
          if (key.toLowerCase().endsWith('id') || key.toLowerCase().endsWith('_id')) {
            const baseName = key.toLowerCase().replace(/id$/, '').replace(/_id$/, '');
            const singularFieldName = fieldName.toLowerCase().replace(/s$/, '');
            
            if (baseName === singularFieldName || baseName.includes(singularFieldName) || singularFieldName.includes(baseName)) {
              foreignKeyId = value;
              relatedTableName = this.inferTableNameFromForeignKey(key);
              break;
            }
          }
        }
      }
    }

    if (!relatedTableName) {
      return null;
    }

    if (isOneToMany) {
      // One-to-many: get all rows from related table where foreignKeyId matches this row's id
      const allRelatedRows = await this.storage.getAllRows(relatedTableName);
      const fkFields = this.inferForeignKeyFields(fieldName, currentTableName);
      
      if (fkFields.length === 0) {
        return [];
      }

      // Try each possible foreign key field
      let matchingRows: any[] = [];
      for (const fkField of fkFields) {
        matchingRows = allRelatedRows.filter(relatedRow => {
          const fkValue = relatedRow[fkField];
          return fkValue === row.id || fkValue === String(row.id) || String(fkValue) === String(row.id);
        });
        
        if (matchingRows.length > 0) {
          break; // Found matches with this FK field
        }
      }

      // Apply selection to each row
      if (selection === true) {
        return matchingRows.map(r => this.removeTableName(r));
      } else {
        // Need to project fields for each matching row
        const projected = await Promise.all(matchingRows.map(r => 
          this.projectFields(r, selection, relatedTableName!)
        ));
        return projected;
      }
    } else {
      // Many-to-one: get single related row
      if (foreignKeyId === null || foreignKeyId === undefined) {
        return null;
      }

      const relatedRow = await this.storage.getRow(relatedTableName, foreignKeyId);

      if (!relatedRow) {
        return null;
      }

      // If selection is just true, return the whole row
      if (selection === true) {
        return this.removeTableName(relatedRow);
      }

      // Otherwise, project the selected fields
      return this.projectFields(relatedRow, selection, relatedTableName);
    }
  }

  private inferForeignKeyFields(fieldName: string, currentTableName: string): string[] {
    // Infer possible foreign key field names in the related table
    // e.g., if fieldName is "posts" and currentTableName is "user", then Post.authorUserId or Post.userId
    
    const singularFieldName = fieldName.toLowerCase().replace(/s$/, '');
    const currentTableSingular = currentTableName.toLowerCase().replace(/s$/, '');
    
    // Common patterns for foreign keys pointing to current table
    const patterns = [
      `author${this.capitalize(currentTableSingular)}Id`,
      `${currentTableSingular}Id`,
      `${currentTableSingular}_id`,
      `owner${this.capitalize(currentTableSingular)}Id`,
      `parent${this.capitalize(currentTableSingular)}Id`,
    ];
    
    return patterns;
  }

  private capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
  }

  private inferTableNameFromField(fieldName: string): string {
    // Infer table name from relationship field name
    // e.g., "users" -> "user", "posts" -> "post"
    const singular = fieldName.toLowerCase().replace(/s$/, '');
    return singular;
  }

  private inferTableNameFromForeignKey(fkField: string): string {
    // Infer table name from foreign key field name
    // e.g., "authorUserId" -> "user", "postId" -> "post"
    const base = fkField.toLowerCase().replace(/id$/, '').replace(/_id$/, '');
    
    // Remove common prefixes
    const withoutPrefix = base.replace(/^(author|owner|parent|child)/, '');
    
    return withoutPrefix || base;
  }

  private removeTableName(row: any): any {
    // Remove the tableName field that we add for IndexedDB storage
    const { tableName, ...rest } = row;
    return rest;
  }
}
