# Remote SQL Execution (libsql)

## Temporary Table Lifecycle

**Important**: When reusing the same database client connection across multiple `batch()` calls, SQLite temporary tables persist between batches. They are only dropped when the connection closes.

### How It Works

According to the [libsql client documentation](https://tursodatabase.github.io/libsql-client-ts/interfaces/Client.html):

- The `batch()` method executes SQL statements within a single transaction
- Each batch operation uses the **same underlying database connection** as the client
- The connection remains open and can be reused for subsequent operations
- Temporary tables created in one batch will persist to the next batch if the same client is reused

### Temporary Tables

SQLite temporary tables (`CREATE TEMP TABLE`) are scoped to the database connection that created them:

- **Temp tables persist across batches** when using the same client connection
- Temp tables are only dropped when the connection closes (not after each batch)
- If you reuse the same client for multiple queries, temp tables from previous batches will still exist
- This can cause errors like `SQLITE_ERROR: table inserted_post already exists` if the same temp table name is used in subsequent batches

### Implications for Pyre

Pyre generates SQL that creates temporary tables for:
- Tracking affected rows in mutations (inserts, updates, deletes)
- Managing nested inserts across multiple tables
- Capturing deleted rows before deletion

**Current Behavior:**
- When tracking affected rows, Pyre does NOT drop temp tables explicitly (to avoid lock errors while result sets are active)
- When NOT tracking affected rows, Pyre drops temp tables explicitly
- **This means temp tables persist across batches when tracking affected rows**, which can cause conflicts

**Solution Needed:**
- Use `CREATE TEMP TABLE IF NOT EXISTS` or `DROP TABLE IF EXISTS` patterns
- Or explicitly drop temp tables after result sets are consumed
- Or use unique temp table names per query execution

### Production Considerations

In production environments:
- The main libsql client should be created once and reused across requests (singleton pattern)
- **Temp tables created in one batch will persist to the next batch** when using the same client
- You must either:
  1. Drop temp tables explicitly after each batch (when safe)
  2. Use unique temp table names per execution
  3. Use `IF NOT EXISTS` / `DROP IF EXISTS` patterns to handle existing tables

## References

- [libsql Client Interface Documentation](https://tursodatabase.github.io/libsql-client-ts/interfaces/Client.html) - Documents that batch() uses a logical connection that closes after completion
