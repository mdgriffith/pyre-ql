import { readFileSync } from 'fs';
import { join } from 'path';
import { createClient } from "@libsql/client";
import type { Client } from "@libsql/client";
import init, { migrate, run_query, sql_is_initialized, sql_introspect, sql_introspect_uninitialized, set_schema } from '../../wasm/pkg/pyre_wasm.js';

// Initialize WASM with Node.js file system
const wasmPath = join(process.cwd(), '..', '..', 'wasm', 'pkg', 'pyre_wasm_bg.wasm');
const wasmBuffer = readFileSync(wasmPath);
await init(wasmBuffer);

// Type definitions matching the Rust structs
interface MigrateInput {
    introspection: Introspection;
    schema_source: string;
}

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
}


async function introspect(db: Client): Promise<Introspection> {
    // First check if the database is initialized
    const isInitializedQuery = sql_is_initialized();
    const isInitializedResult = await db.execute(isInitializedQuery);

    if (isInitializedResult.rows.length === 0) {
        throw new Error("Failed to check if database is initialized");
    }

    const isInitialized = isInitializedResult.rows[0].is_initialized === 1;
    console.log("Is initialized?", isInitialized);


    if (isInitialized) {
        // If initialized, run the introspection query
        console.log("Running introspection query");
        const introspectionQuery = sql_introspect();

        try {
            const introspectionResult = await db.execute(introspectionQuery);
            console.log("Introspection result", introspectionResult);
            if (introspectionResult.rows.length === 0) {
                throw new Error("Failed to get introspection result");
            }
            const introspectionJson = JSON.parse(introspectionResult.rows[0].result as string);
            console.log("Initialized")
            console.log(introspectionResult.rows[0].result);
            return introspectionJson;

        } catch (error) {
            console.error("Error introspecting", error);
            throw error;
        }

    } else {
        const uninitializedIntrospect = sql_introspect_uninitialized();

        console.log("RUNNING UNINITIALIZED INTROSPECTION---\n", uninitializedIntrospect);

        const introspectionResult = await db.execute(uninitializedIntrospect);

        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        console.log("Uninitialized")
        console.log(introspectionResult.rows[0].result);
        return JSON.parse(introspectionResult.rows[0].result as string);

    }
}

/**
 * Calls the WASM migration function with the provided input
 * @param input The migration input containing introspection and schema source
 * @returns A promise that resolves to the SQL migration script or rejects with errors
 */
export async function runMigration(db: Client, schemaSource: string) {
    // Set up console logging for WASM debug output
    const originalConsoleLog = console.log;
    console.log = (...args) => {
        originalConsoleLog('[WASM Debug]', ...args);
    };

    console.time('Migration execution time');
    const result = await migrate("init", schemaSource);
    console.timeEnd('Migration execution time');
    console.log = originalConsoleLog;

    if ("Ok" in result) {
        // Run the SQL for the migration
        try {
            console.log("Running migration");
            console.log(result.Ok.sql);
            console.log("----");
            const migrationResult = await db.batch(result.Ok.sql);
            console.log("Migration result", migrationResult);
            console.log("Migration executed");
            console.log("Marking migration success");
            // await db.execute(result.Ok.mark_success);

            // const tableResult = await db.execute("SELECT * FROM sqlite_master WHERE type='table'");
            // console.log("Table result", tableResult);

            //             const tableResult2 = await db.execute(`SELECT 
            //   CASE 
            //     WHEN EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='_pyre_migrations')
            //     AND EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='_pyre_schema')
            //     THEN 1
            //     ELSE 0
            //   END as is_initialized;`);
            //             console.log("Table result 2", tableResult2);

            // Reset cached schema
            console.log("INTROSPECTING POST MIGRATION")
            const introspection = await introspect(db);
            console.log("SETTING INTROSPECTION");
            await set_schema(introspection);
            console.log("INTROSPECTION SET");
        } catch (error) {
            const markFailure = result.Ok.mark_failure
            // We have to add an error message to the mark failure
            markFailure.args.push(JSON.stringify(error));
            await db.execute(markFailure);
        }
    } else {
        console.error(result.Err);
    }

}


const schemaSource = `
record User {
    accounts      @link(Account.userId)
    posts         @link(Post.authorUserId)
    databaseUsers @link(DatabaseUser.userId)

    // Fields
    id        Int     @id
    name      String?
    status    Status
    createdAt DateTime @default(now)
}

record DatabaseUser {
    id         Int   @id
    databaseId String

    userId Int
    users  @link(userId, User.id)
}

record Account {
    @tablename "accounts"
    users @link(userId, User.id)

    id     Int   @id
    userId Int
    name   String
    status String
}

record Post {
    users @link(authorUserId, User.id)

    id           Int     @id
    createdAt    DateTime @default(now)
    authorUserId Int
    title        String
    content      String
    status       Status
}

type Status
   = Active
   | Inactive
   | Special {
        reason String
     }
   | Special2 {
        reason String
        error  String
     }
`

// Example usage
export async function exampleMigration() {
    // Create an in-memory database
    const db = createClient({
        url: "file::memory:"
    });

    const introspection = await introspect(db);
    console.log("Introspection executed");
    await set_schema(introspection);
    console.log("Schema set");

    await runMigration(db, schemaSource);


    console.log("RUNNING AGAIN")

    // Should skip
    await runMigration(db, schemaSource);



}

// Example of how to use the migration function
// Uncomment to run the example
exampleMigration().catch(console.error);
