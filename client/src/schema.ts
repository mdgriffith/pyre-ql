/**
 * Schema management for Pyre Client
 * Parses introspection JSON to extract relationship information
 */

export interface ForeignKey {
  id: number;
  seq: number;
  table: string;
  from: string;
  to: string;
  on_update?: string;
  on_delete?: string;
  match?: string;
}

export interface Column {
  cid: number;
  name: string;
  type: string;
  notnull: number;
  dflt_value: any;
  pk: number;
}

export interface Table {
  name: string;
  columns: Column[];
  foreign_keys: ForeignKey[];
}

export interface IntrospectionJson {
  tables: Table[];
  migration_state: any;
  schema_source: string;
}

export interface LinkInfo {
  fieldName: string;
  relatedTable: string;
  foreignKeyField: string;
  type: 'many-to-one' | 'one-to-many';
}

export interface RecordInfo {
  name: string;
  tableName: string;
  links: LinkInfo[];
}

export interface Schema {
  records: RecordInfo[];
  schemaSource: string;
}

export class SchemaManager {
  private schema: Schema | null = null;
  private recordMap: Map<string, RecordInfo> = new Map();
  private introspectionJson: IntrospectionJson | null = null;

  async fetchSchema(baseUrl: string): Promise<{ schema: Schema; introspection: IntrospectionJson }> {
    try {
      const response = await fetch(`${baseUrl}/schema`);
      if (!response.ok) {
        throw new Error(`Failed to fetch schema: ${response.status} ${response.statusText}`);
      }
      const introspection: IntrospectionJson = await response.json();

      const schema = this.parseIntrospection(introspection);
      this.schema = schema;
      this.introspectionJson = introspection;
      this.buildRecordMap();
      return { schema, introspection };
    } catch (error) {
      console.error('[PyreClient] Failed to fetch schema:', error);
      throw error;
    }
  }

  getIntrospectionJson(): IntrospectionJson | null {
    return this.introspectionJson;
  }

  setIntrospectionJson(introspection: IntrospectionJson) {
    this.introspectionJson = introspection;
    const schema = this.parseIntrospection(introspection);
    this.schema = schema;
    this.buildRecordMap();
  }

  parseIntrospection(introspection: IntrospectionJson): Schema {
    const records: RecordInfo[] = [];
    const tableMap = new Map<string, Table>();

    // Build table map for quick lookup
    for (const table of introspection.tables) {
      tableMap.set(table.name.toLowerCase(), table);
    }

    // Process each table to build relationships
    for (const table of introspection.tables) {
      const links: LinkInfo[] = [];

      // Process foreign keys for many-to-one relationships
      // If table has FK pointing to another table, that's a many-to-one relationship
      for (const fk of table.foreign_keys) {
        // The FK field name is fk.from, pointing to fk.table
        // This creates a many-to-one relationship: currentTable -> relatedTable
        links.push({
          fieldName: this.singularize(fk.table), // Field name is singular of related table
          relatedTable: fk.table,
          foreignKeyField: fk.from,
          type: 'many-to-one',
        });
      }

      // Find reverse relationships (one-to-many)
      // Look for other tables that have FKs pointing to this table
      for (const otherTable of introspection.tables) {
        if (otherTable.name === table.name) continue;

        for (const fk of otherTable.foreign_keys) {
          if (fk.table.toLowerCase() === table.name.toLowerCase() && fk.to === 'id') {
            // This other table has a FK pointing to our table
            // Create a one-to-many relationship
            links.push({
              fieldName: otherTable.name, // Field name is the related table name
              relatedTable: otherTable.name,
              foreignKeyField: fk.from, // FK field on the related table
              type: 'one-to-many',
            });
          }
        }
      }

      // Infer record name from table name (singularize)
      const recordName = this.singularize(table.name);

      records.push({
        name: recordName,
        tableName: table.name,
        links,
      });
    }

    return {
      records,
      schemaSource: introspection.schema_source || '',
    };
  }

  private pluralize(word: string): string {
    // Simple pluralization - in production you'd want a proper library
    if (word.endsWith('y')) {
      return word.slice(0, -1) + 'ies';
    } else if (word.endsWith('s') || word.endsWith('x') || word.endsWith('z') || word.endsWith('ch') || word.endsWith('sh')) {
      return word + 'es';
    } else {
      return word + 's';
    }
  }

  private singularize(word: string): string {
    // Simple singularization - reverse of pluralization
    if (word.endsWith('ies')) {
      return word.slice(0, -3) + 'y';
    } else if (word.endsWith('es') && (word.endsWith('ses') || word.endsWith('xes') || word.endsWith('zes') || word.endsWith('ches') || word.endsWith('shes'))) {
      return word.slice(0, -2);
    } else if (word.endsWith('s')) {
      return word.slice(0, -1);
    }
    return word;
  }

  private buildRecordMap() {
    this.recordMap.clear();
    if (this.schema) {
      for (const record of this.schema.records) {
        this.recordMap.set(record.name.toLowerCase(), record);
        this.recordMap.set(record.tableName.toLowerCase(), record);
      }
    }
  }

  getSchema(): Schema | null {
    return this.schema;
  }

  setSchema(schema: Schema) {
    this.schema = schema;
    this.buildRecordMap();
  }

  getRecord(recordName: string): RecordInfo | null {
    return this.recordMap.get(recordName.toLowerCase()) || null;
  }

  getRelationshipInfo(
    tableName: string,
    fieldName: string
  ): { type: 'many-to-one' | 'one-to-many' | null; relatedTable: string | null; foreignKeyField: string | null } {
    // Find record by table name
    const record = this.recordMap.get(tableName.toLowerCase());
    if (!record) {
      return { type: null, relatedTable: null, foreignKeyField: null };
    }

    // Check if this field has a link
    const link = record.links.find(l => l.fieldName === fieldName);
    if (link) {
      return {
        type: link.type,
        relatedTable: link.relatedTable,
        foreignKeyField: link.foreignKeyField,
      };
    }

    return { type: null, relatedTable: null, foreignKeyField: null };
  }
}
