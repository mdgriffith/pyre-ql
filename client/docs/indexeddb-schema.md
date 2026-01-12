# IndexedDB Schema Design

## Overview

The Pyre client uses IndexedDB to store synced data locally in the browser. This document describes the schema design and tradeoffs.

## Schema Structure

### Database: `pyre-client` (configurable)

#### Object Stores

1. **`tables`** - Stores table data
   - Key: `[tableName, id]` (composite key)
   - Value: Row object with all fields
   - Indexes:
     - `byTable` - Index on tableName
     - `byUpdatedAt` - Index on updatedAt (for conflict resolution and sorting)

2. **`syncCursor`** - Stores sync cursor state
   - Key: `"cursor"` (single entry)
   - Value: `SyncCursor` object

3. **`metadata`** - Stores metadata
   - Key: `"version"` or other metadata keys
   - Value: Metadata values

## Design Decisions

### Why Composite Keys?

**Approach**: Using `[tableName, id]` as a composite key in a single object store.

**Alternatives Considered**:
1. **Separate object store per table** - Would require dynamic store creation
2. **Single store with string keys** - Less efficient for queries

**Tradeoffs**:
- ✅ **Pros**: 
  - Single store simplifies management
  - Easy to iterate all tables
  - IndexedDB handles composite keys efficiently
- ❌ **Cons**:
  - Slightly more complex key structure
  - Need to filter by tableName for queries

**Decision**: Use composite keys for simplicity and flexibility.

### Index Strategy

**Indexes**:
- `byTable` - Enables efficient filtering by table name
- `byUpdatedAt` - Enables efficient conflict resolution and sorting

**Why these indexes?**
- `byTable`: Essential for querying specific tables
- `byUpdatedAt`: Critical for conflict resolution (newest wins) and sorting queries

### Conflict Resolution

**Strategy**: Always use `updatedAt` field. If a row with the same `id` has a newer `updatedAt`, it replaces the old one.

**Implementation**:
- On sync/catchup: Compare `updatedAt` before writing
- On delta: Compare `updatedAt` before applying
- Index on `updatedAt` enables efficient queries

**Tradeoffs**:
- ✅ Simple and predictable
- ✅ Works well for most use cases
- ❌ Requires `updatedAt` field on all tables (which Pyre guarantees)

### Sync Cursor Storage

**Location**: Separate object store `syncCursor`

**Why separate?**
- Cursor is metadata, not data
- Easier to read/write independently
- Clear separation of concerns

**Structure**: Single entry with key `"cursor"` containing the full `SyncCursor` object.

## Performance Considerations

### Read Performance
- Indexes enable fast lookups by table and updatedAt
- Composite keys allow efficient range queries
- Single store reduces overhead

### Write Performance
- Batch writes during catchup for better performance
- Index updates are handled by IndexedDB automatically

### Storage Efficiency
- Single store reduces metadata overhead
- Composite keys are space-efficient
- Indexes add some overhead but enable fast queries

## Migration Strategy

If the schema needs to change:
1. Increment version number in `openDB`
2. Implement migration in `onupgradeneeded`
3. Handle data transformation if needed

## Example Usage

```typescript
// Reading data
const store = tx.objectStore('tables');
const index = store.index('byTable');
const range = IDBKeyRange.bound([tableName, ''], [tableName, '\uffff']);
const rows = await getAll(index, range);

// Writing data
const store = tx.objectStore('tables');
for (const row of rows) {
  await put(store, [tableName, row.id], row);
}

// Reading cursor
const cursorStore = tx.objectStore('syncCursor');
const cursor = await get(cursorStore, 'cursor');
```
