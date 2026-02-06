# Database Indices Implementation

## Overview

Implemented foreign key indices to optimize OneToMany relationship resolution in the Elm client. This addresses the primary performance bottleneck identified in the performance audit.

## Performance Impact

### Before
- **O(N) table scan** for each OneToMany relationship lookup
- Example: 100 users × 1000 posts scanned = **100,000 iterations**

### After
- **O(1) dict lookup** + O(k) row retrieval (k = related rows)
- Example: 100 users × 1 lookup = **100 operations**
- **~1000x improvement** for typical queries with relationships

## Architecture

### Module: `Db/Index.elm`

Opaque `Index` type that maps foreign key values to lists of row IDs:

```elm
-- Internal structure: Dict String (List Int)
-- Example: { "1": [10, 11, 12], "2": [13, 14] }
--          userId -> [postId, postId, ...]
```

### Key Functions

#### Core Operations
- `empty : Index` - Create empty index
- `insert : IndexKey -> RowId -> Index -> Index` - Add row to index
- `remove : IndexKey -> RowId -> Index -> Index` - Remove row from index
- `lookup : IndexKey -> Index -> List RowId` - O(1) lookup
- `update : { oldKey, newKey, rowId } -> Index -> Index` - Handle FK changes

#### Building & Maintenance
- `buildIndicesFromSchema : SchemaMetadata -> Dict String TableData -> Dict (String, String) Index`
  - Scans schema for OneToMany relationships
  - Builds indices on (targetTable, foreignKeyColumn)
  - Called when initial data loads from IndexedDB

- `updateIndicesFromDelta : SchemaMetadata -> Delta -> Dict (String, String) Index -> Dict (String, String) Index`
  - Incrementally updates indices as deltas arrive from SSE
  - Extracts affected rows and updates relevant indices

- `rebuildFromTable : Dict Int (Dict String Value) -> String -> Index`
  - Builds a single index from scratch
  - Useful for recovery or initialization

### Integration Points

#### 1. Database Structure (`Db.elm`)

Changed from String to Int row IDs:

```elm
type alias Db =
    { tables : Dict String TableData
    , indices : Dict (String, String) Db.Index.Index
    }

-- Before: Dict String (Dict String Value)
-- After:  Dict Int (Dict String Value)
type alias TableData = Dict Int (Dict String Value)
```

#### 2. Index Building

Indices are built:
- **On initial load** when `fromInitialData` is called
- **Incrementally** when `applyDelta` is called

#### 3. Query Execution

The query engine automatically uses indices:

```elm
lookupRowsByForeignKeyIndexed : 
    Dict (String, String) Index 
    -> Dict String TableData 
    -> String  -- table name
    -> String  -- foreign key column
    -> Value   -- foreign key value
    -> Maybe (List (Dict String Value))
```

**Behavior:**
- If index exists: O(1) lookup
- If no index: Falls back to linear scan
- Transparent to query code

#### 4. Message Handling

Updated `Db.Msg` to include `SchemaMetadata` for index operations:

```elm
type Msg
    = FromIndexedDb SchemaMetadata Data.IndexedDb.Incoming
    | DeltaReceived SchemaMetadata Delta
    | PersistDelta (List AffectedRow)
```

## Index Strategy

### What Gets Indexed
- **OneToMany relationships only** (the problematic ones)
- Example: `users.id` ← `posts.user_id` creates index on `(posts, user_id)`

### What Doesn't Get Indexed
- **ManyToOne/OneToOne** - Already O(1) via primary key lookup
- **WHERE clause filters** - Deferred for future optimization
- **Null foreign keys** - Not stored in indices

## Known Limitations & Future Work

### 1. Foreign Key Changes ✅ **FIXED**

**Previous Problem:** Deltas only included new row data, not old values.

**Solution Implemented:** The `applyDelta` function now:
1. Looks up existing rows before upserting new data
2. Compares old and new FK values for indexed columns
3. Generates index update operations only when FK values change
4. Applies updates using `Db.Index.update` to properly remove old entries and add new ones

When a FK changes (e.g., post moves from user 1 to user 2):
- ✅ Post removed from index["1"] (old FK value from existing row)
- ✅ Post added to index["2"] (new FK value from delta)

**Implementation Details:**
- `calculateIndexUpdates`: Compares old vs new row to find FK changes
- `applyIndexUpdates`: Applies the calculated updates to indices
- `IndexUpdate`: Type-safe record tracking what needs to change

**Complexity:** O(D × I) where D = delta rows, I = indices per table (typically 1-5)

### 2. Row Deletions

Currently, deltas don't distinguish between updates and deletes. When a row is deleted, it should be removed from all indices. This requires:
- Delta format to indicate deletion
- Index removal logic in `updateIndicesFromDelta`

### 3. Memory Usage

Each index stores all row IDs for that FK column. For 100k rows, this is significant memory overhead. Acceptable tradeoff for the performance gain, but monitor in production.

### 4. WHERE Clause Indices

Future optimization: Build indices for commonly filtered columns (e.g., `status`, `createdAt`). This would speed up queries like `{ @where: { status: "active" } }`.

## Testing Recommendations

1. **Index correctness**
   - Verify indices match table scans for various FK patterns
   - Test with null foreign keys
   - Test with missing foreign key columns

2. **Performance benchmarks**
   - Measure query time with/without indices
   - Test with various dataset sizes (100, 1k, 10k, 100k rows)
   - Profile memory usage

3. **Incremental updates**
   - Verify indices stay consistent after multiple deltas
   - Test rapid delta streams

4. **Edge cases**
   - Empty tables
   - Tables with no relationships
   - Circular relationships

## Files Changed

- **New:** `client-elm/src/Db/Index.elm` (311 lines) - Index implementation
- **Modified:** `client-elm/src/Db.elm` - Integrated indices, changed to Int IDs
- **Modified:** `client-elm/src/Main.elm` - Pass schema to Db messages
- **New:** `client-elm/docs/INDICES.md` - This document

## Build Status

✅ Compiles successfully
✅ All type signatures match
✅ Query execution uses indices transparently

## Next Steps

1. Monitor performance in real-world usage
2. Implement before/after delta format on server (see limitation #1)
3. Add WHERE clause indices if profiling shows they're needed
4. Consider adding index rebuild command for recovery
