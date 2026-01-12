# Pyre Client

Browser-side TypeScript client for Pyre data synchronization and querying. Stores data in IndexedDB and provides live, reactive queries.

## Features

- ✅ **IndexedDB Storage** - Efficient local data storage
- ✅ **Automatic Sync** - Catchup sync loop until all data is retrieved
- ✅ **WebSocket Support** - Live updates via WebSocket connection
- ✅ **GraphQL-like Queries** - Shape-based query syntax
- ✅ **Filtering & Sorting** - Support for where clauses, sorting, and limits
- ✅ **Relationship Resolution** - Automatic resolution of one-to-many and many-to-one relationships
- ✅ **Conflict Resolution** - Newest `updatedAt` wins
- ✅ **Live Updates** - Queries automatically update when data changes
- ✅ **Retry Logic** - Configurable retry and reconnection strategies
- ✅ **Sync Progress** - Hooks for tracking sync progress

## Installation

```bash
# If publishing as npm package
npm install @pyre/client

# Or copy the client folder into your project
```

## Quick Start

```typescript
import { PyreClient } from '@pyre/client';
// or if using the source directly:
// import { PyreClient } from './client/src';

// Initialize client
const client = new PyreClient({
  baseUrl: 'http://localhost:3000',
  userId: 1,
  dbName: 'my-app-db',
});

// Initialize (connects WebSocket and syncs data)
await client.init((progress) => {
  console.log(`Syncing: ${progress.tablesSynced} tables`);
  if (progress.complete) {
    console.log('Sync complete!');
  }
});

// Query data with live updates
const unsubscribe = client.query({
  posts: {
    id: true,
    title: true,
    '@where': { published: true },
    '@sort': [{ field: 'createdAt', direction: 'desc' }],
    '@limit': 10,
    users: {
      id: true,
      name: true,
    },
  },
}, (data) => {
  console.log('Posts updated:', data.posts);
  // Update your UI here
});

// Later: unsubscribe when done
unsubscribe();
```

## API Reference

### `PyreClient`

Main client class.

#### Constructor

```typescript
new PyreClient(config: ClientConfig)
```

**Config Options:**

- `baseUrl` (required): Server base URL (e.g., `"http://localhost:3000"`)
- `userId` (required): User ID for WebSocket connection
- `dbName` (optional): IndexedDB database name (default: `"pyre-client"`)
- `pageSize` (optional): Page size for catchup requests (default: `1000`)
- `retry` (optional): Retry configuration
  - `maxRetries`: Maximum retries (default: `5`)
  - `initialDelay`: Initial delay in ms (default: `1000`)
  - `maxDelay`: Maximum delay in ms (default: `30000`)
  - `backoffMultiplier`: Exponential backoff multiplier (default: `2`)
- `reconnect` (optional): WebSocket reconnection configuration
  - `initialDelay`: Initial delay in ms (default: `1000`)
  - `maxDelay`: Maximum delay in ms (default: `30000`)
  - `backoffMultiplier`: Exponential backoff multiplier (default: `2`)

#### Methods

##### `init(onProgress?: SyncProgressCallback): Promise<void>`

Initialize the client. Connects WebSocket and performs initial sync.

```typescript
await client.init((progress) => {
  console.log(`Syncing table: ${progress.table}`);
  console.log(`Progress: ${progress.tablesSynced} tables`);
  if (progress.complete) {
    console.log('Sync complete!');
  }
});
```

##### `query(shape: QueryShape, callback: (data: any) => void): Unsubscribe`

Execute a query with live updates. Returns an unsubscribe function.

**Query Shape Syntax:**

```typescript
{
  [tableName]: {
    // Field selections
    field1: true,
    field2: true,
    
    // Nested relationships
    relatedTable: {
      id: true,
      name: true,
    },
    
    // Special directives (prefixed with @)
    '@where': { /* filter conditions */ },
    '@sort': [{ field: 'createdAt', direction: 'desc' }],
    '@limit': 10,
  }
}
```

**Filter Operators:**

- Equality: `{ field: value }`
- Operators: `{ field: { $eq: value, $ne: value, $gt: value, $gte: value, $lt: value, $lte: value, $in: [1, 2, 3] } }`
- AND/OR: `{ $and: [{ field1: value1 }, { field2: value2 }] }`, `{ $or: [...] }`

**Example:**

```typescript
const unsubscribe = client.query({
  posts: {
    id: true,
    title: true,
    '@where': {
      published: true,
      createdAt: { $gte: '2024-01-01' },
    },
    '@sort': [
      { field: 'createdAt', direction: 'desc' },
      { field: 'title', direction: 'asc' },
    ],
    '@limit': 20,
    users: {  // Many-to-one relationship
      id: true,
      name: true,
    },
  },
  users: {
    id: true,
    name: true,
    posts: {  // One-to-many relationship
      id: true,
      title: true,
    },
  },
}, (data) => {
  console.log('Data:', data);
  // data.posts - array of posts
  // data.users - array of users with nested posts
});
```

##### `onSyncProgress(callback: SyncProgressCallback): Unsubscribe`

Register a callback for sync progress updates.

```typescript
const unsubscribe = client.onSyncProgress((progress) => {
  console.log('Sync progress:', progress);
});
```

##### `getSyncStatus(): SyncStatus`

Get current sync status.

```typescript
const status = client.getSyncStatus();
console.log('Syncing:', status.syncing);
console.log('Synced:', status.synced);
console.log('Error:', status.error);
```

##### `disconnect(): void`

Disconnect WebSocket and cleanup.

```typescript
client.disconnect();
```

## React Example

```typescript
import { useEffect, useState } from 'react';
import { PyreClient } from '@pyre/client';
// or if using the source directly:
// import { PyreClient } from './client/src';

function PostsList() {
  const [posts, setPosts] = useState([]);
  const client = usePyreClient(); // Your hook or context

  useEffect(() => {
    const unsubscribe = client.query({
      posts: {
        id: true,
        title: true,
        '@where': { published: true },
        '@sort': [{ field: 'createdAt', direction: 'desc' }],
      },
    }, (data) => {
      setPosts(data.posts);
    });

    return unsubscribe;
  }, [client]);

  return (
    <div>
      {posts.map(post => (
        <div key={post.id}>{post.title}</div>
      ))}
    </div>
  );
}
```

## Project Structure

```
client/
├── src/              # Main source code
│   ├── index.ts      # Main entry point
│   ├── types.ts      # Type definitions
│   ├── storage.ts    # IndexedDB storage layer
│   ├── sync.ts       # Sync/catchup logic
│   ├── websocket.ts  # WebSocket management
│   ├── query.ts      # Query execution
│   └── filter.ts     # Filter evaluation
├── examples/         # Example code
│   └── example.ts    # Usage examples
├── docs/             # Documentation
│   └── indexeddb-schema.md
├── README.md         # This file
├── package.json      # Package configuration
└── tsconfig.json     # TypeScript configuration
```

## Architecture

See [IndexedDB Schema Documentation](./docs/indexeddb-schema.md) for details on the storage layer.

### Data Flow

1. **Initialization**: Client connects WebSocket and performs catchup sync
2. **Sync**: Fetches data from `/sync` endpoint in pages until complete
3. **Storage**: Data stored in IndexedDB with conflict resolution (newest `updatedAt` wins)
4. **Queries**: Read from IndexedDB immediately, update automatically when data changes
5. **Live Updates**: WebSocket deltas update IndexedDB and trigger query callbacks

### Conflict Resolution

When the same row ID is received with different `updatedAt` values, the row with the newer `updatedAt` wins. This ensures clients always have the most recent data.

## License

MIT
