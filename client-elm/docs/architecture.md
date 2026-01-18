# Pyre Elm Client Architecture

## Overview

The Pyre Elm client maintains an in-memory database synchronized with a remote server via Server-Sent Events (SSE) and persists data locally using IndexedDB. The architecture is organized around clear boundaries between communication layers and domain logic.

## Core Principles

1. **Ports live with their communication code** - Port definitions are co-located with the modules that use them, indicating good abstraction boundaries.

2. **Domain logic is separate from I/O** - The `Db` module represents pure domain logic for the in-memory database, while `Data.IndexedDb` and `Data.SSE` handle all external communication.

3. **Messages flow through the domain** - External events (from IndexedDB or SSE) are translated into domain messages that `Db.update` handles.

## Module Responsibilities

### `Data.IndexedDb`
**Purpose**: Communication layer for talking to IndexedDB (browser storage)

**Responsibilities**:
- Define ports for IndexedDB communication (`indexedDbOut`, `receiveIndexedDbMessage`)
- Encode/decode messages to/from IndexedDB
- Types for IndexedDB-specific data:
  - `InitialData` - Data read from IndexedDB on startup
  - `TableData` - In-memory representation of table data

**Outgoing Messages** (Elm → IndexedDB):
- `RequestInitialData` - Request all data from IndexedDB
- `WriteDelta` - Persist affected rows to IndexedDB

**Incoming Messages** (IndexedDB → Elm):
- `InitialDataReceived` - Initial data loaded from IndexedDB

**Note**: Deltas are NOT received from IndexedDB. Deltas come from SSE and are written TO IndexedDB for persistence. Delta types are shared via `Data.Delta`.

### `Data.SSE`
**Purpose**: Communication layer for talking to the SSE system (server synchronization)

**Responsibilities**:
- Define ports for SSE communication (`sseOut`, `receiveSSEMessage`)
- Encode/decode messages to/from SSE
- Types for SSE-specific data:
  - `SSEConfig` - Configuration for connecting to SSE endpoint
  - `SyncProgress` - Progress updates during synchronization

**Outgoing Messages** (Elm → SSE):
- `ConnectSSE` - Establish SSE connection
- `DisconnectSSE` - Close SSE connection

**Incoming Messages** (SSE → Elm):
- `DeltaReceived` - Incremental changes from server (uses `Data.Delta.Delta`)
- `SyncProgressReceived` - Progress updates during sync
- `SSEConnected` - Connection established
- `SSEError` - Connection errors
- `SyncCompleteReceived` - Synchronization complete

**Note**: Deltas originate from the server via SSE. Delta types are shared via `Data.Delta`.

### `Db`
**Purpose**: In-memory database representation and domain logic

**Responsibilities**:
- Represent database state in memory
- Execute queries against in-memory data
- Handle domain messages that update the database
- Persist deltas to IndexedDB when received from SSE
- Define `Db.Msg` type that wraps messages from external systems

**Core Functions**:
- `init : Db` - Create empty database
- `update : Db.Msg -> Db -> (Db, Cmd Db.Msg)` - Update database state and persist deltas
- `executeQuery : SchemaMetadata -> Db -> QueryShape -> QueryResult` - Execute queries
- `fromInitialData : InitialData -> Db` - Initialize from IndexedDB data
- `applyDelta : Db -> Delta -> Db` - Apply SSE delta to database
- `extractAffectedTables : Delta -> List String` - Extract affected table names

**Message Types**:
```elm
type Msg
    = FromIndexedDb Data.IndexedDb.Incoming
    | FromSSE Data.SSE.Incoming
    | PersistDelta (List AffectedRow)  -- Acknowledgment after persistence
```

**Note**: `Db.update` handles messages from both IndexedDB and SSE, translating them into database operations. When a delta is received from SSE, `Db.update` applies it to the database AND returns a command to persist it to IndexedDB. The database is the single source of truth for in-memory state.

### `Db.Query`
**Purpose**: Query type definitions and JSON decoding

**Responsibilities**:
- Define query structure types (`QueryShape`, `QueryField`, `WhereClause`, etc.)
- Decode queries from JSON
- Provide type-safe query representation

**Types**:
- `QueryShape` - Top-level query structure
- `QueryField` - Individual field in query
- `QueryFieldValue` - Field value variants (bool, nested, where, sort, limit)
- `WhereClause` - Filter conditions
- `FilterValue` - Filter value variants
- `SortClause` - Sorting directives
- `SortDirection` - Sort order

**Note**: Query execution logic lives in `Db`, but query types and decoding live here.

### `Data.Delta`
**Purpose**: Shared delta types used by both SSE and IndexedDB

**Responsibilities**:
- Define delta structure types
- Encode/decode deltas to/from JSON
- Provide shared representation for incremental changes

**Types**:
- `Delta` - Complete delta with affected rows and indices
- `AffectedRow` - Individual row change with table name, row data, and headers

**Note**: Deltas come from SSE but are persisted to IndexedDB, so both modules need these types.

### `Data.Schema`
**Purpose**: Schema metadata types

**Responsibilities**:
- Define schema structure types
- Decode schema from JSON
- Provide type-safe schema representation

**Types**:
- `SchemaMetadata` - Complete schema definition
- `TableMetadata` - Table structure and relationships
- `LinkInfo` - Relationship information between tables
- `LinkType` - Relationship type (OneToMany, ManyToOne, OneToOne)
- `LinkTarget` - Target table/column for relationships

### `Data.QueryManager`
**Purpose**: Manages query subscriptions and coordinates query execution

**Responsibilities**:
- Manage collection of active query subscriptions
- Handle query registration, updates, and unregistration
- Execute queries when tables change
- Send query results via ports
- Handle mutation requests (routed to Main for HTTP)

**Core Functions**:
- `init : Model` - Create empty query manager
- `update : Msg -> Model -> (Model, Cmd Msg)` - Update subscription state
- `notifyTablesChanged : SchemaMetadata -> Db -> Model -> List String -> List (Cmd msg)` - Trigger affected queries
- `subscriptions : (Incoming -> msg) -> (String -> msg) -> Sub msg` - Subscribe to incoming messages

**Ports**:
- `queryManagerOut` - Send query results and mutation results
- `receiveQueryManagerMessage` - Receive query/mutation requests

**Note**: Query subscriptions live outside the database. When the database is updated, `Main` tells the query manager what tables changed, and the query manager decides which queries should be refreshed.

### `Data.Value`
**Purpose**: Shared value type representation

**Responsibilities**:
- Define value type used throughout the system
- Encode/decode values to/from JSON
- Provide type-safe value representation

**Types**:
- `Value` - Union type for all possible values (String, Int, Float, Bool, Null)

### `Data.Error`
**Purpose**: Error reporting to JavaScript console

**Responsibilities**:
- Send error messages to JavaScript for console.error logging

**Ports**:
- `errorOut` - Send error strings to JavaScript

**Note**: Used for decode errors and other error conditions that should be logged.

### `Main`
**Purpose**: Application entry point and coordination

**Responsibilities**:
- Initialize the application with flags (schema, SSE config)
- Coordinate between `Db`, `Data.IndexedDb`, `Data.SSE`, and `Data.QueryManager`
- Handle application-level concerns (mutations via HTTP)
- Translate between port messages and domain messages
- Route database updates to query manager for query refresh

**Flow**:
1. Initialize with schema and SSE config
2. Request initial data from IndexedDB
3. Connect to SSE
4. Route incoming messages to `Db.update` (which handles persistence)
5. Notify query manager when tables change
6. Handle mutation HTTP requests
7. Route query manager messages for subscription management

## Message Flow

### Startup
```
Main.init
  → Data.IndexedDb.sendMessage(RequestInitialData)
  → Data.SSE.sendMessage(ConnectSSE)
  → IndexedDB responds with InitialData
  → Db.update(FromIndexedDb(InitialDataReceived))
  → Database initialized
```

### Delta Synchronization
```
SSE receives delta from server
  → Data.SSE receives DeltaReceived via receiveSSEMessage
  → Main routes to Db.update(FromSSE(DeltaReceived))
  → Db applies delta to in-memory state
  → Db.update returns command to persist to IndexedDB
  → Main notifies QueryManager of affected tables
  → QueryManager re-executes affected queries
  → Query results sent via queryManagerOut
```

### Query Execution
```
External code registers query
  → QueryManager receives RegisterQuery via receiveQueryManagerMessage
  → QueryManager.update adds subscription
  → Main executes query via Db.executeQuery
  → Query result sent via queryManagerOut
```

### Mutation
```
External code sends mutation
  → Main sends HTTP request
  → Server responds
  → Mutation result sent via port
  → Delta arrives via SSE (handled as above)
```

## Key Design Decisions

1. **Deltas come from SSE, not IndexedDB** - IndexedDB is purely for persistence. All synchronization happens via SSE. Delta types are shared via `Data.Delta`.

2. **Db.update handles persistence** - When deltas are received from SSE, `Db.update` applies them AND returns a command to persist to IndexedDB. This keeps persistence logic in the domain layer.

3. **Query execution is a Db concern** - Queries run against the in-memory database, not against IndexedDB or the server directly.

4. **Query subscriptions live outside Db** - `Data.QueryManager` manages subscriptions separately from database state. When tables change, `Main` notifies the query manager which tables were affected.

5. **Ports are namespaced** - Each communication module has uniquely named ports (`receiveIndexedDbMessage`, `receiveSSEMessage`, `receiveQueryManagerMessage`) to avoid confusion.

6. **Error handling via explicit Error Msg** - Decode errors and other errors are handled via an explicit `Error` message that sends strings to JavaScript for `console.error` logging.

7. **Types are organized by domain** - Query types in `Db.Query`, schema types in `Data.Schema`, value types in `Data.Value`, delta types in `Data.Delta`.

## Future Considerations

- Consider making `Db` an opaque type to enforce invariants
- Query subscriptions could be managed within `Db` rather than `Main`
- Error handling could be more structured (Result types, error messages)
- Consider separating query execution into `Db.Query.Execution` if it grows
