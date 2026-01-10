import { readFileSync } from 'fs';
import { join } from 'path';
import { createClient } from "@libsql/client";
import type { Client } from "@libsql/client";
import init, {
    migrate,
    query_to_sql,
    sql_is_initialized,
    sql_introspect,
    sql_introspect_uninitialized,
    set_schema,
    get_sync_status_sql,
    get_sync_sql
} from '../../wasm/pkg/pyre_wasm.js';

// Initialize WASM with Node.js file system
const wasmPath = join(process.cwd(), '..', '..', 'wasm', 'pkg', 'pyre_wasm_bg.wasm');
const wasmBuffer = readFileSync(wasmPath);
await init(wasmBuffer);

interface Introspection {
    tables: Array<{
        name: string;
        columns: Array<{
            name: string;
            type: string;
        }>;
    }>;
    migration_state: {
        NoMigrationTable: null;
    } | {
        MigrationTable: {
            migrations: Array<{
                name: string;
            }>;
        };
    };
    schema_source: string;
}

// Sync types matching Rust
interface SyncCursor {
    tables: Record<string, TableCursor>;
}

interface TableCursor {
    last_seen_updated_at: number | null;
    permission_hash: string;
}

interface TableSyncData {
    rows: any[][]; // Array of arrays (compact format)
    headers: string[]; // Column names in order
    permission_hash: string;
    last_seen_updated_at: number | null;
}

interface SyncPageResult {
    tables: Record<string, TableSyncData>;
    has_more: boolean;
}

interface Session {
    fields: Record<string, SessionValue>;
}

type SessionValue = null | number | string | Uint8Array;

async function introspect(db: Client): Promise<Introspection> {
    const isInitializedQuery = sql_is_initialized();
    const isInitializedResult = await db.execute(isInitializedQuery);

    if (isInitializedResult.rows.length === 0) {
        throw new Error("Failed to check if database is initialized");
    }

    const isInitialized = isInitializedResult.rows[0].is_initialized === 1;

    if (isInitialized) {
        const introspectionQuery = sql_introspect();
        const introspectionResult = await db.execute(introspectionQuery);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        const introspectionJson = JSON.parse(introspectionResult.rows[0].result as string);
        return introspectionJson;
    } else {
        const uninitializedIntrospect = sql_introspect_uninitialized();
        const introspectionResult = await db.execute(uninitializedIntrospect);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        return JSON.parse(introspectionResult.rows[0].result as string);
    }
}

async function runMigration(db: Client, schemaSource: string) {
    console.time('Migration execution time');
    const result = migrate("init", schemaSource);
    console.timeEnd('Migration execution time');

    if ("Ok" in result) {
        try {
            console.log("Running migration");
            console.log(result.Ok.sql);
            console.log("----");

            if (result.Ok.sql.length > 0) {
                await db.batch(result.Ok.sql);
                await db.execute(result.Ok.mark_success);
                const introspection = await introspect(db);
                console.log("STORED Introspection", introspection);
                set_schema(introspection);
            } else {
                console.log("No changes, skipping");
            }
        } catch (error) {
            console.log("ERROR", error);
            const markFailure = result.Ok.mark_failure;
            markFailure.args.push(JSON.stringify(error));
            await db.execute(markFailure);
        }
    } else {
        console.log("Error");
        console.error(result.Err);
    }
}


// Main sync function using WASM - single call that returns SQL directly
async function getSyncPage(
    db: Client,
    syncCursor: SyncCursor,
    session: Session,
    pageSize: number = 10
): Promise<SyncPageResult> {
    // Step 1: Get sync status SQL
    const statusSql = get_sync_status_sql(syncCursor, session);

    if (typeof statusSql === 'string' && statusSql.startsWith('Error:')) {
        throw new Error(statusSql);
    }

    console.log("=== Sync Status SQL ===");
    console.log(statusSql);
    console.log("=====================\n");

    // Step 2: Execute sync status SQL
    const statusResult = await db.execute(statusSql as string);

    console.log("=== Sync Status Rows ===");
    console.log(JSON.stringify(statusResult.rows, null, 2));
    console.log("=======================\n");

    // Step 3: Get sync SQL for tables that need syncing
    // Parsing happens internally within get_sync_sql
    // libsql rows are already objects with column names as keys, so we can pass them directly
    const syncSqlResult = get_sync_sql(statusResult.rows, syncCursor, session, pageSize);

    if (typeof syncSqlResult === 'string' && syncSqlResult.startsWith('Error:')) {
        throw new Error(syncSqlResult);
    }

    const sqlResult = typeof syncSqlResult === 'string' ? JSON.parse(syncSqlResult) : syncSqlResult as {
        tables: Array<{
            table_name: string;
            permission_hash: string;
            sql: string[];
            headers: string[];
        }>;
    };

    const result: SyncPageResult = {
        tables: {},
        has_more: false,
    };

    // Collect all SQL statements from all tables for a single batch execution
    // This reduces database round trips from N (one per table) to 1
    const allSqlStatements: string[] = [];
    for (const tableSql of sqlResult.tables) {
        allSqlStatements.push(...tableSql.sql);
    }

    // Execute all SQL statements in a single batch (one round trip instead of N)
    const allQueryResults = await db.batch(allSqlStatements);

    // Process results for each table
    let resultIndex = 0;
    for (const tableSql of sqlResult.tables) {
        const updatedAtIndex = tableSql.headers.indexOf('updatedAt');
        const tableRows: any[][] = [];
        let maxUpdatedAt: number | null = null;

        // Process all query results for this table (usually just one)
        for (const sql of tableSql.sql) {
            const queryResult = allQueryResults[resultIndex++];

            // Verify column order matches headers (for safety)
            if (queryResult.columns.length !== tableSql.headers.length) {
                console.warn(`Column count mismatch for table ${tableSql.table_name}: expected ${tableSql.headers.length}, got ${queryResult.columns.length}`);
            }

            // Cache column array reference (minor optimization)
            const columns = queryResult.columns;
            const rows = queryResult.rows || [];

            // Convert rows to positional arrays and calculate max updatedAt in one pass
            // Only check updatedAtIndex once if it's valid (hoist the check)
            if (updatedAtIndex >= 0) {
                for (const row of rows) {
                    const positionalRow = columns.map((column: string) => row[column]);
                    tableRows.push(positionalRow);

                    // Calculate max updatedAt during conversion
                    const updatedAt = positionalRow[updatedAtIndex];
                    if (updatedAt !== null && updatedAt !== undefined && typeof updatedAt === 'number') {
                        if (maxUpdatedAt === null || updatedAt > maxUpdatedAt) {
                            maxUpdatedAt = updatedAt;
                        }
                    }
                }
            } else {
                // No updatedAt column, just convert rows
                for (const row of rows) {
                    tableRows.push(columns.map((column: string) => row[column]));
                }
            }
        }

        // Check if there's more data (SQL fetches pageSize + 1, so > pageSize means more)
        const hasMoreForTable = tableRows.length > pageSize;
        const finalRows = hasMoreForTable ? tableRows.slice(0, pageSize) : tableRows;

        // Calculate maxUpdatedAt from only the rows we're returning
        // If we sliced, recalculate from returned rows (since rows are ASC, max is the last row)
        let finalMaxUpdatedAt = maxUpdatedAt;
        if (hasMoreForTable && updatedAtIndex >= 0 && finalRows.length > 0) {
            const lastUpdatedAt = finalRows[finalRows.length - 1][updatedAtIndex];
            if (lastUpdatedAt !== null && lastUpdatedAt !== undefined && typeof lastUpdatedAt === 'number') {
                finalMaxUpdatedAt = lastUpdatedAt;
            }
        }

        // Store results
        result.tables[tableSql.table_name] = {
            rows: finalRows,
            headers: tableSql.headers,
            permission_hash: tableSql.permission_hash,
            last_seen_updated_at: finalMaxUpdatedAt,
        };

        if (hasMoreForTable) {
            result.has_more = true;
        }
    }

    return result;
}

// Schema with permissions
const schemaSource = `
session {
    userId Int
    role String
}

record User {
    @tablename "users"
    id        Int     @id
    name      String?
    status    String
    createdAt DateTime @default(now)
}

record Post {
    @tablename "posts"
    id           Int     @id
    createdAt    DateTime @default(now)
    authorUserId Int
    title        String
    content      String
    published    Bool
    @allow(query) { authorUserId = Session.userId || published = true }
    @allow(insert) { authorUserId = Session.userId }
    @allow(update) { authorUserId = Session.userId }
    @allow(delete) { authorUserId = Session.userId }
}
`;

// Seed data using pyre insert queries
async function seedDatabase(db: Client) {
    console.log("Seeding database...");

    // Insert users - one query per user since pyre doesn't support multiple records in one insert
    // DateTime fields with @default(now) will be handled automatically
    const insertUser1 = `
insert CreateUser1 {
    user {
        id = 1
        name = "Alice"
        status = "active"
    }
}
`;

    const insertUser2 = `
insert CreateUser2 {
    user {
        id = 2
        name = "Bob"
        status = "active"
    }
}
`;

    const insertUser3 = `
insert CreateUser3 {
    user {
        id = 3
        name = "Charlie"
        status = "active"
    }
}
`;

    // Insert posts - one query per post
    const insertPost1 = `
insert CreatePost1 {
    post {
        id = 1
        authorUserId = 1
        title = "Alice's First Post"
        content = "This is Alice's first post"
        published = true
    }
}
`;

    const insertPost2 = `
insert CreatePost2 {
    post {
        id = 2
        authorUserId = 1
        title = "Alice's Draft"
        content = "This is a draft"
        published = false
    }
}
`;

    const insertPost3 = `
insert CreatePost3 {
    post {
        id = 3
        authorUserId = 2
        title = "Bob's Post"
        content = "This is Bob's post"
        published = true
    }
}
`;

    const insertPost4 = `
insert CreatePost4 {
    post {
        id = 4
        authorUserId = 3
        title = "Charlie's Post"
        content = "This is Charlie's post"
        published = false
    }
}
`;

    // Execute insert queries
    const queries = [
        insertUser1,
        insertUser2,
        insertUser3,
        insertPost1,
        insertPost2,
        insertPost3,
        insertPost4,
    ];

    for (const query of queries) {
        console.log("Executing query:", query);
        const sql = query_to_sql(query);
        if (sql.Ok) {
            console.log("Generated SQL:", JSON.stringify(sql.Ok, null, 2));
            await db.batch(sql.Ok);
        } else {
            console.error("Query generation failed:", JSON.stringify(sql.Err, null, 2));
            throw new Error(`Failed to generate insert SQL: ${JSON.stringify(sql.Err)}`);
        }
    }

    console.log("Database seeded!");
}

async function initialize(db: Client) {
    const introspection = await introspect(db);
    console.log("Introspection tables:", introspection.tables.length);
    console.log("Introspection schema_source length:", introspection.schema_source?.length || 0);
    set_schema(introspection);
    console.log("Schema set in WASM cache");
}

// Main example
async function exampleSync() {
    console.log("=== Sync Example ===\n");

    // Create an in-memory database
    const db = createClient({
        url: "file::memory:"
    });

    await initialize(db);
    await runMigration(db, schemaSource);
    // Re-introspect after migration to get the schema source
    const introspection = await introspect(db);
    console.log("After migration - Introspection tables:", introspection.tables.length);
    console.log("After migration - Schema source length:", introspection.schema_source?.length || 0);
    set_schema(introspection);
    await seedDatabase(db);

    // Create a session (as user 1 - Alice)
    const session: Session = {
        fields: {
            userId: 1,
            role: "user"
        }
    };

    // Start with empty sync cursor
    const syncCursor: SyncCursor = {
        tables: {}
    };

    console.log("\n=== Calling get_sync_page with empty cursor ===\n");
    const syncResult = await getSyncPage(db, syncCursor, session, 10);

    console.log("Sync Result:");
    console.log(JSON.stringify(syncResult, null, 2));

    console.log("\n=== Summary ===");
    console.log(`Has more: ${syncResult.has_more}`);
    console.log(`Tables synced: ${Object.keys(syncResult.tables).length}`);

    for (const [tableName, tableData] of Object.entries(syncResult.tables)) {
        console.log(`\n${tableName}:`);
        console.log(`  Headers:`, tableData.headers);
        console.log(`  Rows: ${tableData.rows.length}`);
        console.log(`  Permission hash: ${tableData.permission_hash}`);
        console.log(`  Last seen updated_at: ${tableData.last_seen_updated_at}`);
        if (tableData.rows.length > 0) {
            // Convert first row array to object for display
            const sampleRow: Record<string, any> = {};
            tableData.headers.forEach((header, idx) => {
                sampleRow[header] = tableData.rows[0][idx];
            });
            console.log(`  Sample row:`, JSON.stringify(sampleRow, null, 2));
        }
    }
}

// Run the example
exampleSync().catch(console.error);
