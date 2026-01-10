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
    calculate_sync_deltas
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

// Sync deltas types matching Rust
interface AffectedRow {
    table_name: string;
    row: Record<string, any>; // Object with column names as keys
    headers: string[]; // Column names in order
}

interface ConnectedSession {
    session_id: string;
    fields: Record<string, SessionValue>;
}

type SessionValue = null | number | string | Uint8Array;

interface AffectedRowGroup {
    session_ids: string[]; // HashSet serializes as array in JSON
    affected_row_indices: number[];
}

interface SyncDeltasResult {
    all_affected_rows: AffectedRow[];
    groups: AffectedRowGroup[];
}

interface Session {
    fields: Record<string, SessionValue>;
}

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

    // Insert users
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

    // Insert posts
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

// Extract affected rows from mutation result
function extractAffectedRowsFromBatch(batchResults: any[]): AffectedRow[] {
    const affectedRows: AffectedRow[] = [];

    // Look through all result sets for _affectedRows
    for (const resultSet of batchResults) {
        if (!resultSet.columns || !resultSet.rows) continue;

        const affectedRowsColIndex = resultSet.columns.indexOf('_affectedRows');
        if (affectedRowsColIndex === -1) continue;

        for (const row of resultSet.rows) {
            const affectedRowsValue = (row as any)[resultSet.columns[affectedRowsColIndex]];
            if (!affectedRowsValue) continue;

            // Parse the JSON string if needed
            let affectedRowsArray: any[];
            if (typeof affectedRowsValue === 'string') {
                affectedRowsArray = JSON.parse(affectedRowsValue);
            } else if (Array.isArray(affectedRowsValue)) {
                affectedRowsArray = affectedRowsValue;
            } else {
                continue;
            }

            // Process each affected row
            for (const affectedRowData of affectedRowsArray) {
                // Handle both string and object formats
                let rowData: any;
                if (typeof affectedRowData === 'string') {
                    rowData = JSON.parse(affectedRowData);
                } else {
                    rowData = affectedRowData;
                }

                if (rowData.table_name && rowData.row && rowData.headers) {
                    affectedRows.push({
                        table_name: rowData.table_name,
                        row: rowData.row,
                        headers: rowData.headers,
                    });
                }
            }
        }
    }

    return affectedRows;
}

// Main example demonstrating sync deltas
async function exampleSyncDeltas() {
    console.log("=== Sync Deltas Example ===\n");

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

    // Simulate a mutation: Alice publishes her draft post (id=2)
    console.log("\n=== Simulating mutation: Publishing Alice's draft post ===\n");
    const updateQuery = `
update PublishDraft {
    post {
        @where { id = 2 }
        published = true
        id
    }
}
`;

    const updateSql = query_to_sql(updateQuery);
    if (!updateSql.Ok) {
        throw new Error(`Failed to generate update SQL: ${JSON.stringify(updateSql.Err)}`);
    }

    console.log("Update SQL:", JSON.stringify(updateSql.Ok, null, 2));

    // Execute the mutation
    const updateResults = await db.batch(updateSql.Ok.sql);

    // Extract affected rows from the result
    const affectedRows = extractAffectedRowsFromBatch(updateResults);

    console.log("\n=== Affected Rows from Mutation ===");
    console.log(JSON.stringify(affectedRows, null, 2));
    console.log("===================================\n");

    // Define connected sessions (simulating multiple users connected)
    const connectedSessions: ConnectedSession[] = [
        {
            session_id: "session_alice",
            fields: {
                userId: 1,
                role: "user"
            }
        },
        {
            session_id: "session_bob",
            fields: {
                userId: 2,
                role: "user"
            }
        },
        {
            session_id: "session_charlie",
            fields: {
                userId: 3,
                role: "user"
            }
        }
    ];

    console.log("=== Connected Sessions ===");
    console.log(JSON.stringify(connectedSessions, null, 2));
    console.log("==========================\n");

    // Calculate sync deltas
    console.log("=== Calculating Sync Deltas ===\n");
    const deltasResult = calculate_sync_deltas(affectedRows, connectedSessions);

    if (typeof deltasResult === 'string' && deltasResult.startsWith('Error:')) {
        throw new Error(deltasResult);
    }

    const result = typeof deltasResult === 'string' ? JSON.parse(deltasResult) : deltasResult as SyncDeltasResult;

    console.log("=== Sync Deltas Result ===");
    console.log(JSON.stringify(result, null, 2));
    console.log("==========================\n");

    console.log("=== Summary ===");
    console.log(`Total groups: ${result.groups.length}`);
    console.log(`Total unique affected rows: ${result.all_affected_rows.length}`);

    for (const group of result.groups) {
        console.log(`\nSessions: ${group.session_ids.join(', ')}`);
        console.log(`  Affected row indices: ${group.affected_row_indices.join(', ')}`);
        for (const idx of group.affected_row_indices) {
            const affectedRow = result.all_affected_rows[idx];
            console.log(`    - Table: ${affectedRow.table_name}`);
            console.log(`      Row data:`, JSON.stringify(affectedRow.row, null, 6));
        }
    }

    // Expected behavior:
    // - Alice (userId=1) should receive the update because she's the author (authorUserId = Session.userId)
    // - Bob and Charlie should also receive it because published = true (public post)
    // So all three sessions should receive the delta
    console.log("\n=== Expected Behavior ===");
    console.log("All sessions should receive the delta because:");
    console.log("  - Alice: authorUserId (1) = Session.userId (1)");
    console.log("  - Bob & Charlie: published = true (public post)");
}

// Run the example
exampleSyncDeltas().catch(console.error);

