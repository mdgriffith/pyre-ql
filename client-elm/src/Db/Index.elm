module Db.Index exposing
    ( Index
    , IndexKey
    , RowId
    , empty
    , insert
    , remove
    , update
    , lookup
    , rebuildFromTable
    , buildIndicesFromSchema
    , updateIndicesFromDelta
    )

{-| Index module for efficient foreign key lookups.

An Index maps foreign key values to lists of row IDs, enabling O(1) lookups
instead of O(N) table scans for relationship resolution.


# Performance Improvement

Before indices: O(N) table scan for each OneToMany relationship
After indices: O(1) lookup + O(k) row retrieval where k = number of related rows

Example: Query 100 users with their posts (each user has ~50 posts)

    Before: 100 parent lookups × 1000 posts scanned = 100,000 iterations
    After: 100 lookups × 1 dict access = 100 operations


# Index Building Strategy

Indices are built for OneToMany relationships only, as these are the problematic ones.
ManyToOne and OneToOne relationships use primary key lookups which are already O(1).

Indices are:

  - Built from scratch when initial data loads from IndexedDB
  - Updated incrementally as deltas arrive from SSE

-}

import Data.Delta exposing (Delta, TableGroup)
import Data.Schema exposing (LinkType(..), SchemaMetadata)
import Data.Value exposing (Value)
import Dict exposing (Dict)


{-| Opaque index type.

Internally, this is a Dict from IndexKey (foreign key value) to a list of RowIds.

-}
type Index
    = Index (Dict String (List Int))


{-| The foreign key value (e.g., "1", "2" for user_id values).
-}
type alias IndexKey =
    String


{-| Row identifier (corresponds to SQLite rowid).
-}
type alias RowId =
    Int


{-| Create an empty index.
-}
empty : Index
empty =
    Index Dict.empty


{-| Insert a single row ID under a foreign key value.

If the IndexKey is already present, the RowId is added to the list.

-}
insert : IndexKey -> RowId -> Index -> Index
insert key rowId (Index dict) =
    Index <|
        Dict.update key
            (\maybeList ->
                case maybeList of
                    Just list ->
                        if List.member rowId list then
                            Just list

                        else
                            Just (rowId :: list)

                    Nothing ->
                        Just [ rowId ]
            )
            dict


{-| Remove a row ID from under a foreign key value.

If the list becomes empty, the key is removed from the index.

-}
remove : IndexKey -> RowId -> Index -> Index
remove key rowId (Index dict) =
    Index <|
        Dict.update key
            (\maybeList ->
                case maybeList of
                    Just list ->
                        let
                            filtered =
                                List.filter (\id -> id /= rowId) list
                        in
                        if List.isEmpty filtered then
                            Nothing

                        else
                            Just filtered

                    Nothing ->
                        Nothing
            )
            dict


{-| Update an index when a foreign key value changes.

This handles the case where a row's foreign key changes from oldKey to newKey.
If oldKey is Nothing, only insert. If newKey is Nothing, only remove.

**TODO: Improve Delta Format**

Currently, deltas only include the new row data, not the old values. This means
we can't properly remove the old foreign key entry from the index when a FK changes.

Example problematic scenario:

    - Post 10 has user_id: 1 (indexed under "1")
    - Delta arrives: Post 10 now has user_id: 2
    - We add Post 10 to index["2"], but can't remove it from index["1"]

Solution: Enhance the server's delta format to include both before and after data:

    type alias AffectedRow =
        { tableName : String
        , before : Maybe (Dict String Value)  -- Old row data
        , after : Dict String Value           -- New row data
        , headers : List String
        }

This would allow proper index updates when foreign keys change.

-}
update : { oldKey : Maybe IndexKey, newKey : Maybe IndexKey, rowId : RowId } -> Index -> Index
update { oldKey, newKey, rowId } index =
    let
        afterRemove =
            case oldKey of
                Just key ->
                    remove key rowId index

                Nothing ->
                    index
    in
    case newKey of
        Just key ->
            insert key rowId afterRemove

        Nothing ->
            afterRemove


{-| Look up all row IDs associated with a foreign key value.

Returns an empty list if the key is not found.

-}
lookup : IndexKey -> Index -> List RowId
lookup key (Index dict) =
    Dict.get key dict
        |> Maybe.withDefault []


{-| Rebuild an index from scratch for a specific column in a table.

This scans all rows in the table and builds an index on the specified column.

-}
rebuildFromTable : Dict Int (Dict String Value) -> String -> Index
rebuildFromTable tableData columnName =
    Dict.foldl
        (\rowId row acc ->
            case Dict.get columnName row of
                Just value ->
                    case valueToIndexKey value of
                        Just key ->
                            insert key rowId acc

                        Nothing ->
                            -- Null foreign key, skip
                            acc

                Nothing ->
                    -- Column doesn't exist in this row, skip
                    acc
        )
        empty
        tableData


{-| Build all necessary indices from schema metadata.

This creates indices for OneToMany relationships (the problematic ones),
as those require foreign key lookups that would otherwise be O(N) table scans.

Returns a Dict keyed by (tableName, columnName).

-}
buildIndicesFromSchema : SchemaMetadata -> Dict String (Dict Int (Dict String Value)) -> Dict ( String, String ) Index
buildIndicesFromSchema schema tables =
    Dict.foldl
        (\tableName tableMeta acc ->
            -- For each table, look at its links
            Dict.foldl
                (\linkName linkInfo innerAcc ->
                    case linkInfo.type_ of
                        OneToMany ->
                            -- Build index on the target table's foreign key column
                            case Dict.get linkInfo.to.table tables of
                                Just targetTable ->
                                    let
                                        index =
                                            rebuildFromTable targetTable linkInfo.to.column

                                        indexKey =
                                            ( linkInfo.to.table, linkInfo.to.column )
                                    in
                                    Dict.insert indexKey index innerAcc

                                Nothing ->
                                    innerAcc

                        _ ->
                            -- ManyToOne and OneToOne use primary key lookups (already O(1))
                            innerAcc
                )
                acc
                tableMeta.links
        )
        Dict.empty
        schema.tables


{-| Update all indices based on a delta.

This incrementally updates indices when new data arrives via SSE.

**Current Limitation:** Only handles inserts and updates where the FK value stays the same.
Cannot properly handle FK value changes (e.g., post moves from user 1 to user 2) because
deltas don't include the old row data.

See the `update` function documentation for the proposed delta format improvement.

-}
updateIndicesFromDelta : SchemaMetadata -> Delta -> Dict ( String, String ) Index -> Dict ( String, String ) Index
updateIndicesFromDelta schema delta indices =
    List.foldl
        (\tableGroup acc ->
            updateIndicesForTableGroup schema tableGroup acc
        )
        indices
        delta.tableGroups


{-| Update indices for all rows in a table group.
-}
updateIndicesForTableGroup : SchemaMetadata -> TableGroup -> Dict ( String, String ) Index -> Dict ( String, String ) Index
updateIndicesForTableGroup schema tableGroup indices =
    let
        tableName =
            tableGroup.tableName

        headers =
            tableGroup.headers
    in
    List.foldl
        (\rowArray acc ->
            let
                rowObj =
                    rowArrayToObject headers rowArray

                rowId =
                    getRowIdFromRow rowObj
            in
            case rowId of
                Just id ->
                    -- Update all indices for this table
                    updateIndicesForTable schema tableName id rowObj acc

                Nothing ->
                    -- Can't index without an ID
                    acc
        )
        indices
        tableGroup.rows


{-| Convert a row array to a row object using headers.
-}
rowArrayToObject : List String -> List Value -> Dict String Value
rowArrayToObject headers values =
    List.map2 Tuple.pair headers values
        |> Dict.fromList


{-| Update all indices that involve a specific table.

For each index on this table, extract the foreign key value and update the index.

-}
updateIndicesForTable : SchemaMetadata -> String -> RowId -> Dict String Value -> Dict ( String, String ) Index -> Dict ( String, String ) Index
updateIndicesForTable schema tableName rowId row indices =
    -- Find all indices for this table by checking which (table, column) pairs exist
    Dict.foldl
        (\( idxTableName, idxColumnName ) index acc ->
            if idxTableName == tableName then
                -- This index is for our table, update it
                case Dict.get idxColumnName row of
                    Just value ->
                        case valueToIndexKey value of
                            Just key ->
                                let
                                    updatedIndex =
                                        insert key rowId index
                                in
                                Dict.insert ( idxTableName, idxColumnName ) updatedIndex acc

                            Nothing ->
                                -- Null foreign key, don't index
                                acc

                    Nothing ->
                        -- Column not in row, skip
                        acc

            else
                acc
        )
        indices
        indices


{-| Extract row ID from a row dictionary.
-}
getRowIdFromRow : Dict String Value -> Maybe Int
getRowIdFromRow row =
    case Dict.get "id" row of
        Just (Data.Value.IntValue i) ->
            Just i

        _ ->
            Nothing


{-| Convert a Value to an IndexKey (String).

Returns Nothing for Null values (we don't index null foreign keys).

-}
valueToIndexKey : Value -> Maybe String
valueToIndexKey value =
    case value of
        Data.Value.IntValue i ->
            Just (String.fromInt i)

        Data.Value.StringValue s ->
            Just s

        Data.Value.NullValue ->
            Nothing

        _ ->
            -- Other types shouldn't be used as foreign keys
            Nothing
