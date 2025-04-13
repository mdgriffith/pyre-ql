import { readFileSync } from 'fs';
import { join } from 'path';
import init, { migrate } from '../../wasm/pkg/pyre_wasm.js';

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

/**
 * Calls the WASM migration function with the provided input
 * @param input The migration input containing introspection and schema source
 * @returns A promise that resolves to the SQL migration script or rejects with errors
 */
export async function runMigration(introspection: Introspection, schemaSource: string): Promise<string> {
    // const inputJson = JSON.stringify(introspection);
    const result = await migrate(introspection, schemaSource);
    const parsedResult = JSON.parse(result);

    if ('errors' in parsedResult) {
        throw new Error(`Migration failed: ${JSON.stringify(parsedResult.errors)}`);
    }

    return parsedResult.sql;
}

const emptyIntrospection: Introspection = {
    tables: [],
    migration_state: { NoMigrationTable: null }
};

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
    try {
        const sql = await runMigration(emptyIntrospection, schemaSource);
        console.log('Generated SQL:', sql);
        return sql;
    } catch (error) {
        console.error('Migration failed:', error);
        throw error;
    }
}

// Example of how to use the migration function
// Uncomment to run the example
exampleMigration().catch(console.error);
