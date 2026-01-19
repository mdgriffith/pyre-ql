# Fine-Grained Query Reactivity

## Overview

This document describes the implementation plan for fine-grained query reactivity in the Elm client. This optimization addresses one of the largest remaining performance bottlenecks: unnecessary query re-executions when unrelated data changes.

## Current Architecture (Table-Level Reactivity)

### How It Works Today

1. Delta arrives via SSE containing changed rows
2. Extract affected table names using `Db.extractAffectedTables`
3. For each active query subscription, check if it uses any affected table
4. If yes → re-execute the entire query regardless of which specific rows changed

### The Problem

```elm
-- Scenario:
-- Query: users WHERE role = 'admin' (returns 5 admin users)
-- Delta: user id=999 (role='user') updates their email
-- 
-- Current behavior: Re-executes entire admin query (scans all users)
-- Cost: O(N) where N = total users in database
-- Result: Same 5 admin users (wasted computation)
```

**Impact at scale:**
- 100 active query subscriptions
- Single row update to one table
- All 100 queries using that table re-execute
- Cost: O(100 × result_set_size)

**Real-world example:**
- User updates their profile
- Triggers re-execution of:
  - Admin dashboard queries (user not in result set)
  - Other users' profile queries (unrelated user)
  - Report queries with date filters (user created months ago)

## Proposed Architecture (Row-Level Reactivity)

### Core Concept

Track which specific row IDs each query depends on, then only re-execute when those exact rows change (or when new rows might enter the result set).

### Key Components

#### 1. Result Set Tracking

```elm
type alias QuerySubscription =
    { queryId : String
    , query : Db.Query.Query
    , input : Encode.Value
    , callbackPort : String
    , resultRowIds : Dict String (Set Int)  -- NEW
    -- Maps query field name → Set of row IDs in current result
    }
```

**Example:**
```elm
-- Query: { users: { id: true, posts: true } }
-- Result: 3 users (ids: 1, 2, 3)
resultRowIds = Dict.fromList [ ("users", Set.fromList [1, 2, 3]) ]
```

#### 2. Smart Delta Filtering

```elm
type ReExecuteDecision
    = NoReExecute              -- Delta doesn't affect this query
    | ReExecuteFull            -- Need full re-execution
    | ReExecutePatch (Set Int) -- Can patch specific rows (future optimization)

shouldReExecuteQuery : 
    SchemaMetadata 
    -> QuerySubscription 
    -> Delta 
    -> ReExecuteDecision
```

**Decision tree:**

1. **Extract changed row IDs from delta**
   - Delta contains table groups with rows
   - Each row has an `id` field
   - Collect all changed IDs per table

2. **Quick rejection: Table overlap check**
   ```
   Query uses tables: [users]
   Delta affects tables: [posts]
   → NoReExecute
   ```

3. **Row intersection check**
   ```
   Query result contains user IDs: {1, 2, 3}
   Delta changes user IDs: {999}
   No intersection → NoReExecute
   ```

4. **Operation type analysis**
   - **Inserts**: Might match WHERE clause → ReExecuteFull
   - **Updates**: Check if change affects filter → depends on analysis
   - **Deletes**: If row in result set → ReExecuteFull

5. **WHERE clause impact analysis** (Phase 3)
   ```elm
   Query: WHERE role = 'admin'
   Delta: user_id=999, role changed from 'user' to 'user' (email updated)
   
   → Check if 'role' field changed → No
   → NoReExecute
   ```

#### 3. WHERE Clause Analysis

```elm
doesChangeAffectWhereClause : 
    WhereClause 
    -> Dict String Value  -- old row
    -> Dict String Value  -- new row
    -> Bool
```

**Algorithm:**
1. Extract all fields referenced in WHERE clause
   - Example: `WHERE role = 'admin' AND status = 'active'` → fields: [role, status]
2. Compare old vs new values for those fields
3. If any changed → return True (might affect result set membership)
4. Otherwise → return False (update doesn't affect filtering)

**Examples:**

| Query | Old Row | New Row | Fields Changed | Affects WHERE? |
|-------|---------|---------|----------------|----------------|
| `role = 'admin'` | `{role: 'user'}` | `{role: 'admin'}` | role | ✅ Yes (entering) |
| `role = 'admin'` | `{role: 'admin'}` | `{role: 'user'}` | role | ✅ Yes (leaving) |
| `role = 'admin'` | `{role: 'user', email: 'a@b.com'}` | `{role: 'user', email: 'c@d.com'}` | email | ❌ No |
| `status = 'active'` | `{status: 'active', name: 'Alice'}` | `{status: 'active', name: 'Alicia'}` | name | ❌ No |

## Implementation Phases

### Phase 1: Foundation ✅ (High Impact, Low Risk)

**Goal:** Infrastructure for tracking result row IDs

**Changes:**

1. **Update QuerySubscription type** (`Data/QueryManager.elm`)
   ```elm
   type alias QuerySubscription =
       { queryId : String
       , query : Db.Query.Query
       , input : Encode.Value
       , callbackPort : String
       , resultRowIds : Dict String (Set Int)  -- NEW
       }
   ```

2. **Modify executeQuery to track row IDs** (`Db.elm`)
   ```elm
   executeQueryWithTracking : 
       SchemaMetadata 
       -> Db 
       -> Query 
       -> { results : Dict String (List (Dict String Value))
          , rowIds : Dict String (Set Int)
          }
   ```

3. **Store row IDs when queries execute** (`Main.elm`, `Data/QueryManager.elm`)
   - Update subscription's `resultRowIds` after each execution
   - Initialize empty on registration

**Testing:**
- Run queries with various filters
- Verify row IDs are correctly captured
- Check edge cases (empty results, nested queries)

**Expected outcome:** No performance change yet, but infrastructure ready

---

### Phase 2: Basic Row-Level Filtering ✅ (High Impact, Medium Complexity)

**Goal:** Skip re-execution when changed rows not in result set

**Changes:**

1. **Implement shouldReExecuteQuery** (`Data/QueryManager.elm`)
   ```elm
   shouldReExecuteQuery : 
       SchemaMetadata 
       -> QuerySubscription 
       -> Delta 
       -> ReExecuteDecision
   ```
   
   **Initial logic (conservative):**
   - Check if delta tables overlap with query tables
   - Check if changed row IDs overlap with result row IDs
   - If overlap exists → ReExecuteFull (safe default)
   - If no overlap → NoReExecute

2. **Add helper: extractChangedRowIds** (`Data/Delta.elm` or `Data/QueryManager.elm`)
   ```elm
   extractChangedRowIds : Delta -> Dict String (Set Int)
   -- Maps table name → Set of changed row IDs
   ```

3. **Update notifyTablesChanged** (`Data/QueryManager.elm`)
   - Replace simple table name check with `shouldReExecuteQuery`
   - Only re-execute if decision is ReExecuteFull

**Testing:**
- Query for users 1, 2, 3
- Update user 999
- Verify query does NOT re-execute
- Update user 2
- Verify query DOES re-execute

**Expected outcome:** 10-20x reduction in unnecessary re-executions

---

### Phase 3: WHERE Clause Analysis (Medium Impact, High Complexity)

**Goal:** Detect when updates don't affect WHERE clause filters

**Changes:**

1. **Implement doesChangeAffectWhereClause** (new module `Db/ReactivityAnalysis.elm`)
   ```elm
   doesChangeAffectWhereClause : 
       WhereClause 
       -> Dict String Value  -- old row
       -> Dict String Value  -- new row
       -> Bool
   ```

2. **Extract referenced fields from WHERE clause**
   ```elm
   extractWhereClauseFields : WhereClause -> Set String
   -- Example: {role: {$eq: "admin"}, status: {$eq: "active"}}
   -- Returns: Set ["role", "status"]
   ```

3. **Get old row values before applying delta** (`Data/QueryManager.elm`)
   - When delta arrives, look up existing rows from current DB state
   - Compare with new values from delta
   - Only count as "affecting" if filtered fields changed

4. **Update shouldReExecuteQuery logic**
   - For updates: Call doesChangeAffectWhereClause
   - If filtered fields unchanged AND row already in result → NoReExecute
   - If filtered fields changed → ReExecuteFull (might enter/leave result set)

**Edge cases:**
- Inserts: Always ReExecuteFull (might match WHERE)
- Deletes: If in result set → ReExecuteFull
- Complex operators ($and, $or): Conservative approach initially
- LIMIT/SORT: If present + insert → always ReExecuteFull

**Testing:**
- Query: `WHERE role = 'admin'`
- Update admin user's email → no re-execution
- Update admin user's role → re-execution
- Update non-admin to admin → re-execution
- Insert new admin user → re-execution

**Expected outcome:** 50-100x reduction for filtered queries

---

### Phase 4: Advanced Optimizations (Future)

**Goal:** Further performance improvements

**Potential enhancements:**

1. **Patch-based updates** (instead of full re-execution)
   ```elm
   ReExecutePatch (Set Int)  -- Just re-fetch these specific rows
   ```
   - For updates to rows in result set where WHERE clause unaffected
   - Fetch updated rows, replace in result
   - Much faster than re-executing entire query

2. **Operation type tracking** (requires server changes)
   ```json
   {
     "table_name": "posts",
     "operation": "update",  // or "insert", "delete"
     "headers": ["id", "title"],
     "rows": [[10, "New Title"]]
   }
   ```
   - Explicit operation type makes decision tree simpler
   - Can optimize insert vs update vs delete differently

3. **Relationship dependency tracking**
   ```elm
   type alias QuerySubscription =
       { ...
       , relatedRowIds : Dict String (Dict String (Set Int))
       -- Example: { "users": { "posts": Set [10,11,12] } }
       }
   ```
   - Track which related rows were loaded for nested selections
   - If post changes and it's in tracked set → re-execute parent query
   - Handles: `{ users: { posts: true } }` when posts change

4. **Index-assisted filtering**
   - Use existing foreign key indices to speed up "which queries affected?"
   - Reverse index: row_id → Set of query_ids that include it

## Performance Analysis

### Before (Current)

```
Scenario: 100 active subscriptions, 1 user updates email

→ Extract affected tables: [users]
→ Check all 100 subscriptions
→ 100 subscriptions use "users" table
→ 100 full query re-executions
→ Cost: O(100 × N) where N = avg result set size
```

### After Phase 2 (Row-Level Filtering)

```
Same scenario

→ Extract affected tables: [users]
→ Extract changed row IDs: {999}
→ Check all 100 subscriptions:
   - 95 don't have row 999 in result set → NoReExecute
   - 5 have row 999 in result set → ReExecuteFull
→ 5 full query re-executions
→ Cost: O(5 × N)
→ Improvement: 20x
```

### After Phase 3 (WHERE Clause Analysis)

```
Same scenario, queries have WHERE clauses

→ Extract affected tables: [users]
→ Extract changed row IDs: {999}
→ Check 5 subscriptions that include row 999:
   - 4 queries: WHERE includes non-email fields → check change
     - Email changed, role unchanged → NoReExecute (4 queries)
   - 1 query: No WHERE clause → ReExecuteFull (1 query)
→ 1 full query re-execution
→ Cost: O(1 × N)
→ Improvement: 100x
```

## Data Structure Changes

### QuerySubscription (Before)

```elm
type alias QuerySubscription =
    { queryId : String
    , query : Db.Query.Query
    , input : Encode.Value
    , callbackPort : String
    }
```

### QuerySubscription (After Phase 2)

```elm
type alias QuerySubscription =
    { queryId : String
    , query : Db.Query.Query
    , input : Encode.Value
    , callbackPort : String
    , resultRowIds : Dict String (Set Int)
    -- Maps query field → Set of row IDs in current result
    -- Example: { "users": Set [1,2,3], "posts": Set [10,11,12] }
    }
```

### Query Execution Return Type

```elm
-- Before
executeQuery : SchemaMetadata -> Db -> Query -> Dict String (List (Dict String Value))

-- After (alternative signature for tracking)
executeQueryWithTracking : 
    SchemaMetadata 
    -> Db 
    -> Query 
    -> { results : Dict String (List (Dict String Value))
       , rowIds : Dict String (Set Int)
       }
```

## Memory Overhead Analysis

**Per subscription:**
- Row IDs storage: ~8 bytes per row ID (Int in Set)
- Dict overhead: ~50 bytes base + key storage
- Example: 100 rows in result = ~800 bytes

**System-wide:**
- 1000 active subscriptions
- Average 50 rows per result set
- Total: 1000 × 50 × 8 bytes = 400 KB

**Conclusion:** Negligible overhead for massive performance gain

## Edge Cases & Challenges

### 1. Nested Queries (Relationships)

**Challenge:**
```elm
-- Query includes nested data
{ users: { id: true, posts: true } }

-- A post changes, not a user
-- Current row tracking only tracks user IDs
-- Won't detect that post change affects query
```

**Solution (Phase 4):**
- Track related row IDs separately
- Store mapping: parent_field → related_field → Set row_ids
- Check both direct and related changes

### 2. Complex WHERE Clauses

**Challenge:**
```elm
WHERE {
  $or: [
    { status: "active" },
    { priority: { $gt: 5 } }
  ]
}
```

**Solution:**
- Extract all fields from nested operators ($and, $or)
- Conservative: If any referenced field changes → might affect result
- Optimization: Could evaluate full condition with old/new values

### 3. LIMIT and SORT

**Challenge:**
```elm
-- Query: top 10 users by created_at
{ users: { @sort: [{field: "created_at", direction: "desc"}], @limit: 10 } }

-- Row outside top 10 changes, might now be in top 10
```

**Solution:**
- If query has LIMIT/SORT + delta is insert → always ReExecuteFull
- If query has LIMIT/SORT + delta updates sorted field → always ReExecuteFull
- Otherwise can use row-level filtering

### 4. Deletes

**Challenge:**
- Current delta format doesn't distinguish deletes from updates
- Need to know if a row was removed from DB

**Solution (requires server change):**
- Add operation type to delta format
- Client can check: if delete + row in result set → ReExecuteFull

### 5. Consistency During Updates

**Challenge:**
- Delta arrives
- Mid-processing, query executes
- Might see partial state

**Solution:**
- Elm's architecture is single-threaded, no race conditions
- Updates are atomic within the update function
- Not a real issue in practice

## Testing Strategy

### Unit Tests

1. **Row ID extraction**
   - Query with various filters → verify correct IDs captured
   - Empty results → empty set
   - Multiple fields → separate sets per field

2. **Delta change extraction**
   - Single table delta → correct row IDs
   - Multiple tables → grouped by table
   - Edge cases: empty delta, missing IDs

3. **Row intersection**
   - Overlapping sets → true
   - Disjoint sets → false
   - Empty result set → false

### Integration Tests

1. **Full flow scenarios**
   - Register query → execute → update unrelated row → verify no re-execution
   - Register query → execute → update related row → verify re-execution
   - Multiple queries → verify each evaluated independently

2. **WHERE clause scenarios**
   - Filtered field changes → re-execution
   - Unfiltered field changes → no re-execution
   - Complex filters ($and, $or) → correct behavior

### Performance Benchmarks

1. **Measure before/after**
   - 100 subscriptions
   - Single row update
   - Count actual re-executions

2. **Stress test**
   - 1000 subscriptions
   - Rapid delta stream
   - Memory usage
   - Latency per delta

## Migration Path

### No Breaking Changes

This optimization is entirely internal to the client. External APIs remain unchanged:
- Query format: unchanged
- Delta format: unchanged (Phase 1-3)
- Result format: unchanged

### Rollout Strategy

1. **Phase 1:** Deploy with tracking but no behavior change
   - Monitor memory usage
   - Verify row IDs collected correctly

2. **Phase 2:** Enable row-level filtering
   - Feature flag if needed
   - Monitor re-execution counts
   - Verify no correctness regressions

3. **Phase 3:** Enable WHERE clause analysis
   - Gradual rollout per filter type
   - Start with simple equality, expand to operators

## Success Metrics

### Quantitative

- **Query re-execution rate**: Expect 10-100x reduction
- **Client CPU usage**: Expect 30-50% reduction during high delta throughput
- **Memory overhead**: Expect <1MB increase for typical workloads
- **Latency P95**: Expect 20-40% improvement (fewer query backlogs)

### Qualitative

- No correctness regressions (query results remain accurate)
- No stale data issues
- Reduced battery usage on mobile (less CPU churn)

## Future Enhancements

1. **Query result caching**
   - Cache parsed queries and execution plans
   - Speeds up repeated executions

2. **Batch delta processing**
   - If multiple deltas arrive in quick succession
   - Batch analyze and execute once

3. **Smart prefetching**
   - Based on result set analysis
   - Prefetch likely-needed rows into cache

4. **Query optimization hints**
   - Allow app to mark queries as "static" (rarely changes)
   - Or "high priority" (always eager re-execute)

## References

- Current implementation: `client-elm/src/Db.elm`, `client-elm/src/Data/QueryManager.elm`
- Index optimization: `client-elm/docs/INDICES.md`
- Delta format: `client-elm/docs/DELTA_FORMAT.md`
- Related discussion: Original performance audit notes

## Conclusion

Fine-grained query reactivity transforms the client from **table-level** to **row-level** precision. By tracking which specific rows each query depends on and intelligently analyzing changes, we can reduce unnecessary query re-executions by 10-100x.

The phased implementation approach allows incremental delivery of value while managing complexity. Each phase is independently testable and delivers meaningful performance improvements.

This optimization, combined with the existing foreign key index optimization, positions the client to handle high-throughput real-time sync scenarios efficiently.
