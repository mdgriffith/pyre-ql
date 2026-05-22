# Optimistic Mutations

Pyre optimistic mutations let the client apply a mutation locally before the server confirms it.

## In-Flight Mutation

Each optimistic mutation is tracked as an in-flight record:

```ts
type InFlightMutation = {
  requestId: string
  mutationId: string
  forward: TableDelta[]
  inverse: TableDelta[]
}
```

- `forward` is applied immediately to the local table cache.
- `inverse` reverts the local change if the server rejects the mutation.
- `requestId` ties the local optimistic state to the server response.

## Flow

1. Generated client code builds `forward` deltas from the mutation input.
2. Store the in-flight mutation.
3. Build `inverse` deltas from the current local rows.
4. Apply `forward` locally and update active queries.
5. Send the existing mutation request to the server.
6. Resolve the in-flight mutation from the server response.

## Local-Only Deltas

Optimistic deltas are not part of the mutation protocol. They are generated client-side and used only to update the local cache.

- The server never receives client-generated deltas.
- The server never trusts client-generated deltas.
- The server still receives only the normal mutation input at `POST /db/:mutationId`.
- Server authority still comes from normal mutation execution plus live sync/catchup.

Generated optimistic deltas are predictions. They should cover the local rows Pyre can safely derive from the mutation input.

## Server Request

V1 uses the existing mutation endpoint unchanged:

```http
POST /db/:mutationId
```

The HTTP body remains the normal mutation input. No optimistic delta data is sent.

## Server Result

- Success: drop the in-flight mutation. No immediate local change is needed.
- Failure: apply `inverse`, drop the in-flight mutation, and surface the error.

The mutation's normal response is still available to app code, but it is not used to update the local table cache.

## Server Confirmation

Server confirmation happens through the existing live sync or catchup path. When the authoritative table delta arrives, the normal sync code applies it to the local cache and updates active queries.

This also handles common server amendments, such as `createdAt`, `updatedAt`, derived values, or normalized data, as long as the optimistic row uses the same primary key as the server row.

## IDs

Optimistic creates are simplest when synced tables use client-generatable IDs, such as UUIDv7 or ULID.

Server-assigned integer IDs require temp IDs, ID remapping, relationship rewrites, and more complex reconciliation. Pyre should only require client-generated IDs for tables that opt into local optimistic creates, not for every synced table.
