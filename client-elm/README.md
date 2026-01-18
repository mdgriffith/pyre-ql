# Pyre Elm Client

A headless Elm application for Pyre data synchronization and querying. This client manages data in memory and communicates with IndexedDB and SSE via TypeScript ports.

## Architecture

- **Elm (`src/`)**: Manages in-memory state, executes queries, handles deltas, and sends mutations
- **TypeScript (`src-ts/`)**: Handles IndexedDB operations and SSE communication via ports

## Setup

1. Install dependencies:
```bash
npm install
```

2. Build Elm:
```bash
npm run build
# or for development:
npm run dev
```

3. Build TypeScript:
```bash
npm run typecheck
```

## Usage

### Initialization

```typescript
import { initPyreElmClient } from './src-ts/index';
import schemaMetadata from './generated/client/node/schema';

// Initialize Elm app (assuming you've compiled it)
const elmApp = Elm.Main.init({
  flags: schemaMetadata
});

// Initialize the TypeScript bridge
initPyreElmClient(elmApp, 'pyre-client');

// Connect SSE (this will be triggered from Elm)
elmApp.ports.connectSSE.subscribe((config) => {
  // SSE connection is handled by TypeScript bridge
});
```

### Registering Queries

```typescript
import { Encode } from 'elm-ts-interop'; // or similar

// Register a query
elmApp.ports.receiveRegisterQuery.send([
  'query-1',                    // queryId
  queryShapeJson,               // QueryShape as JSON
  inputJson,                    // Input as JSON
  'callback-port-1'            // callbackPort identifier
]);

// Subscribe to query results
elmApp.ports.queryResult.subscribe(([callbackPort, result]) => {
  // Route result to appropriate callback based on callbackPort
  if (callbackPort === 'callback-port-1') {
    handleQueryResult(result);
  }
});
```

### Updating Query Input

```typescript
// Update query input (re-executes query)
elmApp.ports.receiveUpdateQueryInput.send([
  'query-1',     // queryId
  newInputJson   // New input as JSON
]);
```

### Sending Mutations

```typescript
// Send a mutation
elmApp.ports.receiveSendMutation.send([
  'mutation-hash',           // hash
  'http://localhost:3000',  // baseUrl
  inputJson                  // Input as JSON
]);

// Subscribe to mutation results
elmApp.ports.mutationResult.subscribe(([hash, result]) => {
  case result of
    Ok response -> handleSuccess(response);
    Err error -> handleError(error);
});
```

## Ports

### Outgoing (Elm -> TypeScript)

- `requestInitialData`: Request all data from IndexedDB
- `writeDelta`: Write a delta to IndexedDB
- `connectSSE`: Connect to SSE endpoint
- `disconnectSSE`: Disconnect from SSE
- `queryResult`: Send query results (callbackPort, result)
- `mutationResult`: Send mutation results (hash, result)

### Incoming (TypeScript -> Elm)

- `receiveInitialData`: Receive all data from IndexedDB
- `receiveDelta`: Receive a synced delta
- `receiveSyncProgress`: Receive sync progress updates
- `receiveSyncComplete`: Receive sync complete notification
- `receiveSSEConnected`: Receive SSE connection confirmation
- `receiveSSEError`: Receive SSE error
- `receiveRegisterQuery`: Register a new query (queryId, queryShape, input, callbackPort)
- `receiveUpdateQueryInput`: Update query input (queryId, newInput)
- `receiveUnregisterQuery`: Unregister a query (queryId)
- `receiveSendMutation`: Send a mutation (hash, baseUrl, input)

## Features

- ✅ In-memory data management
- ✅ Query execution against local state
- ✅ Automatic query re-execution on data changes
- ✅ Delta application and persistence
- ✅ HTTP mutation support
- ✅ SSE connection management
- ✅ IndexedDB persistence

## Limitations

- Query results with nested relationships are simplified (full JSON encoding needed)
- `$in` operator in filters needs proper list handling
- Dynamic port creation not supported (uses single queryResult port with routing)
