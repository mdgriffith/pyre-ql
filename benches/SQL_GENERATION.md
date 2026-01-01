# Pyre SQL Generation: Avoiding Common Pitfalls

This document explains common SQL pitfalls when querying nested relationships and how Pyre's SQL generation avoids them.

## 1. The Cartesian Product Problem

When querying multiple one-to-many relationships with JOINs, you create a **Cartesian product** that explodes the result set.

### Example

With 1000 users, 5 posts per user, and 5 accounts per user:

```sql
SELECT u.id, u.name, p.title, a.name
FROM users u
LEFT JOIN posts p ON u.id = p.authorUserId
LEFT JOIN accounts a ON u.id = a.userId
```

**Problem:** This creates **25,000 rows** (1000 × 5 × 5). Each user's data is duplicated 25 times.

### Why It's Bad

- **Exponential scaling:** More relationships = exponentially more rows
- **Memory waste:** Processing 25x more data than needed
- **Slow sorting:** Must sort all duplicated rows before `LIMIT`
- **Hangs on large datasets:** Benchmarks show naive JOINs can hang indefinitely

## 2. Pyre's Solution: CTEs with Pre-Aggregation

Pyre avoids Cartesian products by:
1. Querying each relationship separately using CTEs
2. Aggregating JSON at each level before joining
3. Joining pre-aggregated results (1:1:1) instead of raw tables (1:many:many)

### Example: Pyre-Generated SQL

For this query:
```pyre
query GetUsersWithPostsAndAccounts {
    user {
        id
        name
        posts { id, title }
        accounts { id, name }
    }
}
```

Pyre generates:
```sql
WITH selected_user AS (
    SELECT id, name FROM users
),
selected_posts AS (
    SELECT 
        authorUserId,
        jsonb_group_array(jsonb_object('id', id, 'title', title)) AS posts_json
    FROM posts
    WHERE authorUserId IN (SELECT id FROM selected_user)
    GROUP BY authorUserId
),
selected_accounts AS (
    SELECT 
        userId,
        jsonb_group_array(jsonb_object('id', id, 'name', name)) AS accounts_json
    FROM accounts
    WHERE userId IN (SELECT id FROM selected_user)
    GROUP BY userId
)
SELECT json_object(
    'user', json_group_array(json_object(
        'id', u.id,
        'name', u.name,
        'posts', COALESCE(p.posts_json, '[]'),
        'accounts', COALESCE(a.accounts_json, '[]')
    ))
) AS result
FROM selected_user u
LEFT JOIN selected_posts p ON u.id = p.authorUserId
LEFT JOIN selected_accounts a ON u.id = a.userId
```

**Note:** Pyre uses `jsonb_*` functions (`jsonb_group_array`, `jsonb_object`) for intermediate CTEs and `json_*` functions (`json_group_array`, `json_object`) for the final result.

**Result:** Only 1000 rows (one per user) instead of 25,000.

### Key Pattern

Pyre uses `WHERE ... IN (SELECT ...)` instead of JOINs for nested relationships:

```rust
// Use WHERE ... IN (SELECT ...) - JOIN optimization causes memory issues in some cases
```

This queries each relationship independently and aggregates before combining.

## 3. JSON Processing in SQL vs Application Code

Pyre processes JSON in SQL rather than application code. Benchmarks show this is significantly faster.

### Benchmark Results

For nested queries (users with posts):

| Approach | Time | Notes |
|----------|------|-------|
| **Pyre SQL JSON** | 6.79 ms | JSON built in SQL |
| **Naive JOIN** | 5.38 ms | Raw rows, no JSON (but creates Cartesian product) |
| **JOIN + App JSON** | 12.25 ms | JOIN + application-side JSON composition |

### Why SQL JSON Processing is Better

1. **Faster:** ~2x faster than application-side processing (6.79ms vs 12.25ms)
2. **Less data transfer:** Only final JSON transferred, not all intermediate rows
3. **Database optimization:** SQLite's JSON functions are optimized
4. **Single round-trip:** All processing happens in one query

### The Trade-off

There's a small overhead compared to raw JOINs (6.79ms vs 5.38ms), but:
- Raw JOINs create Cartesian products (hangs on complex queries)
- Application-side JSON is much slower (12.25ms)
- SQL JSON gives you the best of both: correct structure + good performance

## 4. Summary

**Pyre's approach:**
- ✅ Avoids Cartesian products automatically
- ✅ Scales linearly instead of exponentially  
- ✅ Processes JSON efficiently in SQL (faster than app code)
- ✅ Returns nested structure matching your query

**Trade-offs:**
- Slightly slower than raw JOINs (but raw JOINs break on complex queries)
- Generated SQL is more complex (but hidden from users)
- Less flexible than hand-written SQL (but safer by default)

## Conclusion

Pyre's SQL generation provides a **good default** that avoids common performance pitfalls. By processing JSON in SQL and avoiding Cartesian products, Pyre ensures nested queries are both correct and performant without requiring SQL expertise.
