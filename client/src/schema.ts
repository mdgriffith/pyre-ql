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

export interface LinkJson {
  table_name: string;
  field_name: string;
  foreign_key_field: string;
  related_table: string;
  link_type: 'many-to-one' | 'one-to-many';
}

export interface IntrospectionJson {
  tables: Table[];
  migration_state: any;
  schema_source: string;
  links: LinkJson[]; // Parsed link information from server
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
  private schemaMetadata: import('./types').SchemaMetadata | null = null;

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
    const tableSet = new Set<string>();

    // Build table set for quick lookup
    for (const table of introspection.tables) {
      tableSet.add(table.name.toLowerCase());
    }

    // Group links by table name
    const linksByTable = new Map<string, LinkInfo[]>();
    for (const link of introspection.links) {
      const tableName = link.table_name.toLowerCase();
      if (!linksByTable.has(tableName)) {
        linksByTable.set(tableName, []);
      }
      linksByTable.get(tableName)!.push({
        fieldName: link.field_name,
        relatedTable: link.related_table,
        foreignKeyField: link.foreign_key_field,
        type: link.link_type as 'many-to-one' | 'one-to-many',
      });
    }

    // Process each table to build records
    for (const table of introspection.tables) {
      const links = linksByTable.get(table.name.toLowerCase()) || [];

      // Use table name as record name (singularized would be ideal but not critical)
      const recordName = table.name;

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

  setSchemaMetadata(metadata: import('./types').SchemaMetadata) {
    this.schemaMetadata = metadata;
    // Build record map from metadata
    this.recordMap.clear();
    for (const [tableName, tableMeta] of Object.entries(metadata.tables)) {
      const links: LinkInfo[] = [];
      for (const [fieldName, relInfo] of Object.entries(tableMeta.relationships)) {
        if (relInfo.type) {
          links.push({
            fieldName,
            relatedTable: relInfo.relatedTable || '',
            foreignKeyField: relInfo.foreignKeyField || '',
            type: relInfo.type,
          });
        }
      }
      this.recordMap.set(tableName.toLowerCase(), {
        name: tableMeta.name,
        tableName: tableMeta.name,
        links,
      });
    }
  }

  getTableNameFromQueryField(queryFieldName: string): string | null {
    if (!this.schemaMetadata) {
      return null;
    }
    return this.schemaMetadata.queryFieldToTable[queryFieldName] || null;
  }

  getSchemaMetadata(): import('./types').SchemaMetadata | null {
    return this.schemaMetadata;
  }

  getRelationshipInfo(
    tableName: string,
    fieldName: string
  ): { type: 'many-to-one' | 'one-to-many' | null; relatedTable: string | null; foreignKeyField: string | null } {
    // First try schema metadata (preferred)
    if (this.schemaMetadata) {
      const tableMeta = this.schemaMetadata.tables[tableName];
      if (tableMeta) {
        const relInfo = tableMeta.relationships[fieldName];
        if (relInfo) {
          return {
            type: relInfo.type,
            relatedTable: relInfo.relatedTable,
            foreignKeyField: relInfo.foreignKeyField,
          };
        }
      }
    }

    // Fallback to record map (for backwards compatibility)
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
