import { createClient } from "@libsql/client";
import type { Client } from "@libsql/client";
import { readFileSync } from "fs";
import { join } from "path";
import { existsSync } from "fs";
import init, {
    migrate,
    sql_is_initialized,
    sql_introspect,
    sql_introspect_uninitialized,
    set_schema,
    seed_database,
} from "../../../wasm/pkg/pyre_wasm.js";

// Initialize WASM
const wasmPath = join(process.cwd(), "..", "..", "wasm", "pkg", "pyre_wasm_bg.wasm");
const wasmBuffer = readFileSync(wasmPath);
await init({ wasm: wasmBuffer });

const DB_PATH = join(process.cwd(), "test.db");

async function runMigration() {
    const db = createClient({
        url: `file:${DB_PATH}`,
    });

    // Read schema file
    const schemaPath = join(process.cwd(), "pyre", "schema.pyre");
    if (!existsSync(schemaPath)) {
        throw new Error(`Schema file not found: ${schemaPath}`);
    }
    const schemaSource = readFileSync(schemaPath, "utf-8");

    // Load schema into WASM cache BEFORE migration (required for migrate to work)
    // Check if database is initialized and introspect accordingly
    await loadSchema(db);

    // Run migration using WASM
    console.log("Running migration...");
    const migrationResult = migrate("init", schemaSource);

    if ("Ok" in migrationResult) {
        try {
            if (migrationResult.Ok.sql.length > 0) {
                await db.batch(migrationResult.Ok.sql);
                await db.execute(migrationResult.Ok.mark_success);
                console.log("Migration completed");
            } else {
                console.log("No migration changes needed");
            }
        } catch (error) {
            console.error("Migration execution failed:", error);
            const markFailure = migrationResult.Ok.mark_failure;
            markFailure.args.push(JSON.stringify(error));
            await db.execute(markFailure);
            throw error;
        }
    } else {
        console.error("Migration failed:", migrationResult.Err);
        throw new Error(`Migration error: ${JSON.stringify(migrationResult.Err)}`);
    }

    // Reload schema into WASM cache after migration (to update cache with new state)
    await loadSchema(db);
}

async function loadSchema(db: Client) {
    const isInitializedQuery = sql_is_initialized();
    const isInitializedResult = await db.execute(isInitializedQuery);

    if (isInitializedResult.rows.length === 0) {
        throw new Error("Failed to check if database is initialized");
    }

    const isInitialized = isInitializedResult.rows[0].is_initialized === 1;

    let introspection;
    if (isInitialized) {
        const introspectionQuery = sql_introspect();
        const introspectionResult = await db.execute(introspectionQuery);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        introspection = JSON.parse(introspectionResult.rows[0].result as string);
    } else {
        const uninitializedIntrospect = sql_introspect_uninitialized();
        const introspectionResult = await db.execute(uninitializedIntrospect);
        if (introspectionResult.rows.length === 0) {
            throw new Error("Failed to get introspection result");
        }
        introspection = JSON.parse(introspectionResult.rows[0].result as string);
    }

    set_schema(introspection);
    console.log("Schema loaded into WASM cache");
}

async function seedDatabase() {
    const db = createClient({
        url: `file:${DB_PATH}`,
    });

    console.log("Seeding database...");

    // Read schema file for seed_database
    const schemaPath = join(process.cwd(), "pyre", "schema.pyre");
    if (!existsSync(schemaPath)) {
        throw new Error(`Schema file not found: ${schemaPath}`);
    }
    const schemaSource = readFileSync(schemaPath, "utf-8");

    // Use seed_database from WASM to generate seed SQL
    const seedResult = seed_database(schemaSource, null);

    // Check if result is an error string
    if (typeof seedResult === 'string' && seedResult.startsWith('Error:')) {
        throw new Error(`Failed to generate seed SQL: ${seedResult}`);
    }

    // seedResult should be a SeedSql object with sql array
    if (!seedResult.sql || !Array.isArray(seedResult.sql)) {
        throw new Error(`Unexpected seed result format: ${JSON.stringify(seedResult)}`);
    }

    // Execute the seed SQL statements
    if (seedResult.sql.length > 0) {
        await db.batch(seedResult.sql);
        console.log(`Seeded database with ${seedResult.sql.length} SQL statements`);
    } else {
        console.log("No seed data to insert");
    }

    console.log("Seeding completed");
}

// Main execution
try {
    await runMigration();
    await seedDatabase();
    console.log("✅ Initialization completed successfully");
} catch (error) {
    console.error("❌ Initialization failed:", error);
    process.exit(1);
}
