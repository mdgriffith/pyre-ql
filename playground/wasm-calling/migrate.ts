import { readFileSync } from 'fs';
import { join } from 'path';
import { createClient } from "@libsql/client";
import type { Client } from "@libsql/client";
import init, { migrate, query_to_sql, sql_is_initialized, sql_introspect, sql_introspect_uninitialized, set_schema } from '../../wasm/pkg/pyre_wasm.js';

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
}


async function introspect(db: Client): Promise<Introspection> {
    // First check if the database is initialized
    const isInitializedQuery = sql_is_initialized();
    const isInitializedResult = await db.execute(isInitializedQuery);

    if (isInitializedResult.rows.length === 0) {
        throw new Error("Failed to check if database is initialized");
    }

    const isInitialized = isInitializedResult.rows[0].is_initialized === 1;


    if (isInitialized) {
        // If initialized, run the introspection query
        // console.log("Running introspection query");
        const introspectionQuery = sql_introspect();

        try {
            const introspectionResult = await db.execute(introspectionQuery);
            if (introspectionResult.rows.length === 0) {
                throw new Error("Failed to get introspection result");
            }
            const introspectionJson = JSON.parse(introspectionResult.rows[0].result as string);

            return introspectionJson;

        } catch (error) {
            console.error("Error introspecting", error);
            throw error;
        }

    } else {
        const uninitializedIntrospect = sql_introspect_uninitialized();

        // console.log("RUNNING UNINITIALIZED INTROSPECTION---\n", uninitializedIntrospect);

        const introspectionResult = await db.execute(uninitializedIntrospect);

        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        // console.log("Uninitialized")
        // console.log(introspectionResult.rows[0].result);
        return JSON.parse(introspectionResult.rows[0].result as string);

    }
}

/**
 * Calls the WASM migration function with the provided input
 * @param input The migration input containing introspection and schema source
 * @returns A promise that resolves to the SQL migration script or rejects with errors
 */
export async function runMigration(db: Client, schemaSource: string) {


    console.time('Migration execution time');
    const result = migrate("init", schemaSource);
    console.timeEnd('Migration execution time');

    if ("Ok" in result) {
        // Run the SQL for the migration
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
            const markFailure = result.Ok.mark_failure
            // We have to add an error message to the mark failure
            markFailure.args.push(JSON.stringify(error));
            await db.execute(markFailure);
        }
    } else {
        console.log("Error")
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


const seed = `
insert Seed {
    user {
        name = "Porkchop"
        status = Active
        accounts {
            name = "My account"
            status = "Untyped status"
        }
        posts {
            title = "My first post"
            content = "This is my first post"
            status = Active
        }
        databaseUsers {
            databaseId = "user.db"
        }

    }
}
`

async function initialize(db: Client) {
    const introspection = await introspect(db);
    set_schema(introspection);
    // Doesn't actually run the migration
    // But convinces the wasm to fully load.
    migrate("init", schemaSource);

}

async function run(db: Client, pyre_query: string) {

    const sql = query_to_sql(pyre_query);
    if (sql.Ok) {
        console.log(sql.Ok);
        const result = await db.batch(sql.Ok);
        console.log("Retrieved");
        console.log(result);
        for (const set of result) {
            console.log("SET", set);
            for (const row of set.rows) {
                console.log(row);
            }
        }
    } else {
        console.log("Error");
        console.log(sql.Err);
    }
}

// Example usage
export async function exampleMigration() {

    // Create an in-memory database
    const db = createClient({
        url: "file::memory:"
    });

    await initialize(db);

    await runMigration(db, schemaSource);

    // Should skip
    await runMigration(db, schemaSource);

    // Seed the database
    run(db, seed);

}

// Example of how to use the migration function
// Uncomment to run the example
exampleMigration().catch(console.error);
