# Remote SQL Execution (libsql)

## Temporary Table Lifecycle

When using `@libsql/client` with the `batch()` method, temporary tables are automatically cleaned up when the batch's logical connection closes.

### How It Works

According to the [libsql client documentation](https://tursodatabase.github.io/libsql-client-ts/interfaces/Client.html):

- The `batch()` method executes SQL statements within a single transaction
- Each batch operation uses a **logical database connection** that is distinct from the main client connection
- After the batch operation completes, **this logical connection is closed automatically**
- The main client connection remains open and can be reused for subsequent operations

### Temporary Tables

SQLite temporary tables (`CREATE TEMP TABLE`) are scoped to the database connection that created them. Since each `batch()` call uses its own logical connection that closes after completion:

- Temporary tables created during a batch are automatically dropped when the batch's logical connection closes
- No explicit `DROP TABLE` statements are needed for cleanup
- Temp tables will not persist across different batch executions, even if the main client is reused

### Implications for Pyre

Pyre generates SQL that creates temporary tables for:
- Tracking affected rows in mutations (inserts, updates, deletes)
- Managing nested inserts across multiple tables
- Capturing deleted rows before deletion

Since libsql automatically cleans up these temp tables when the batch completes, Pyre does not need to generate explicit `DROP TABLE` statements. This avoids potential database lock errors that can occur when trying to drop tables while result sets are still being consumed.

### Production Considerations

In production environments:
- The main libsql client should be created once and reused across requests (singleton pattern)
- Each request uses `client.batch()` which creates a temporary logical connection
- Temp tables created in one batch are isolated from other batches, even on the same client
- Automatic cleanup ensures no temp table pollution across requests

## References

- [libsql Client Interface Documentation](https://tursodatabase.github.io/libsql-client-ts/interfaces/Client.html) - Documents that batch() uses a logical connection that closes after completion
