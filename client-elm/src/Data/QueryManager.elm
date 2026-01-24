port module Data.QueryManager exposing (Incoming(..), Message, Model, Msg(..), QuerySubscription, ReExecuteDecision(..), decodeIncoming, doesChangeAffectWhereClause, encodeMessage, extractChangedRowIds, extractWhereClauseFields, init, mutationResult, notifyTablesChanged, queryResult, receiveIncoming, sendMessage, shouldReExecuteQuery, subscriptions, update)

import Data.Delta
import Data.Schema
import Data.Value exposing (Value)
import Db
import Db.Query
import Dict exposing (Dict)
import Json.Decode as Decode
import Json.Encode as Encode
import Set exposing (Set)



-- Model


type alias Model =
    { subscriptions : Dict String QuerySubscription
    }


type alias QuerySubscription =
    { queryId : String
    , query : Db.Query.Query
    , input : Encode.Value
    , callbackPort : String
    , resultRowIds : Dict String (Set Int)
    }



-- Messages


type Msg
    = IncomingReceived Incoming


type Incoming
    = RegisterQuery String Db.Query.Query Encode.Value String -- queryId, query, input, callbackPort
    | UpdateQueryInput String (Maybe Db.Query.Query) Encode.Value -- queryId, query, newInput
    | UnregisterQuery String -- queryId
    | SendMutation String String Encode.Value -- hash, baseUrl, input


type Message
    = QueryResult String Encode.Value -- callbackPort, result
    | MutationResult String (Result String Encode.Value) -- hash, result



-- Init


init : Model
init =
    { subscriptions = Dict.empty
    }



-- Update


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        IncomingReceived incoming ->
            handleIncoming incoming model


handleIncoming : Incoming -> Model -> ( Model, Cmd Msg )
handleIncoming incoming model =
    case incoming of
        RegisterQuery queryId query input callbackPort ->
            let
                subscription =
                    QuerySubscription queryId query input callbackPort Dict.empty

                updatedSubscriptions =
                    Dict.insert queryId subscription model.subscriptions
            in
            ( { model | subscriptions = updatedSubscriptions }
            , Cmd.none
            )

        UpdateQueryInput queryId maybeQuery newInput ->
            case Dict.get queryId model.subscriptions of
                Just subscription ->
                    let
                        updatedQuery =
                            Maybe.withDefault subscription.query maybeQuery

                        updatedSubscription =
                            { subscription | input = newInput, query = updatedQuery }

                        updatedSubscriptions =
                            Dict.insert queryId updatedSubscription model.subscriptions
                    in
                    ( { model | subscriptions = updatedSubscriptions }
                    , Cmd.none
                    )

                Nothing ->
                    ( model, Cmd.none )

        UnregisterQuery queryId ->
            ( { model | subscriptions = Dict.remove queryId model.subscriptions }
            , Cmd.none
            )

        SendMutation _ _ _ ->
            -- Mutations are handled by Main, not QueryManager
            ( model, Cmd.none )



-- Notify that tables have changed (with fine-grained reactivity)


notifyTablesChanged : Data.Schema.SchemaMetadata -> Db.Db -> Model -> Data.Delta.Delta -> ( Model, List (Cmd msg) )
notifyTablesChanged schema db model delta =
    Dict.foldl
        (\queryId subscription ( accModel, accCmds ) ->
            let
                -- Use fine-grained reactivity to decide if re-execution is needed
                decision =
                    shouldReExecuteQuery schema db subscription delta
            in
            case decision of
                ReExecuteFull ->
                    let
                        executionResult =
                            Db.executeQueryWithTracking schema db subscription.query

                        resultJson =
                            encodeQueryResult executionResult.results

                        -- Update subscription with new row IDs
                        updatedSubscription =
                            { subscription | resultRowIds = executionResult.rowIds }

                        updatedSubscriptions =
                            Dict.insert queryId updatedSubscription accModel.subscriptions

                        updatedModel =
                            { accModel | subscriptions = updatedSubscriptions }
                    in
                    ( updatedModel
                    , queryResult subscription.callbackPort resultJson :: accCmds
                    )

                NoReExecute ->
                    -- Delta doesn't affect this query, skip re-execution
                    ( accModel, accCmds )
        )
        ( model, [] )
        model.subscriptions


extractQueryTables : Data.Schema.SchemaMetadata -> Db.Query.Query -> List String
extractQueryTables schema query =
    Dict.foldl
        (\queryFieldName _ acc ->
            case Dict.get queryFieldName schema.queryFieldToTable of
                Just tableName ->
                    if List.member tableName acc then
                        acc

                    else
                        tableName :: acc

                Nothing ->
                    acc
        )
        []
        query


encodeQueryResult : Dict String (List (Dict String Value)) -> Encode.Value
encodeQueryResult result =
    Encode.dict identity
        (\rows ->
            Encode.list (\row -> Encode.dict identity Data.Value.encodeValue row) rows
        )
        result



-- Fine-grained reactivity helpers


{-| Extract row IDs that changed from a delta, grouped by table name.
-}
extractChangedRowIds : Data.Delta.Delta -> Dict String (Set Int)
extractChangedRowIds delta =
    List.foldl
        (\tableGroup acc ->
            let
                changedIds =
                    List.filterMap
                        (\row ->
                            -- Row is a list of values, first one should be id
                            case row of
                                (Data.Value.IntValue id) :: _ ->
                                    Just id

                                _ ->
                                    Nothing
                        )
                        tableGroup.rows
                        |> Set.fromList
            in
            Dict.insert tableGroup.tableName changedIds acc
        )
        Dict.empty
        delta.tableGroups


type ReExecuteDecision
    = NoReExecute
    | ReExecuteFull


{-| Extract all field names referenced in a WHERE clause.

This includes fields in all nested conditions ($and, $or, operators, etc.)

-}
extractWhereClauseFields : Db.Query.WhereClause -> Set String
extractWhereClauseFields whereClause =
    Dict.foldl
        (\fieldName filterValue acc ->
            case filterValue of
                Db.Query.FilterValueAnd clauses ->
                    -- Recursively extract from nested AND clauses
                    List.foldl
                        (\clause innerAcc ->
                            Set.union innerAcc (extractWhereClauseFields clause)
                        )
                        acc
                        clauses

                Db.Query.FilterValueOr clauses ->
                    -- Recursively extract from nested OR clauses
                    List.foldl
                        (\clause innerAcc ->
                            Set.union innerAcc (extractWhereClauseFields clause)
                        )
                        acc
                        clauses

                Db.Query.FilterValueOperators _ ->
                    -- Field is being filtered
                    Set.insert fieldName acc

                Db.Query.FilterValueSimple _ ->
                    -- Field is being filtered
                    Set.insert fieldName acc

                Db.Query.FilterValueNull ->
                    -- Field is being filtered
                    Set.insert fieldName acc
        )
        Set.empty
        whereClause


{-| Check if a change to a row affects a WHERE clause.

Compares the old and new row values for fields referenced in the WHERE clause.
Returns True if any filtered field changed, False otherwise.

-}
doesChangeAffectWhereClause : Db.Query.WhereClause -> Dict String Value -> Dict String Value -> Bool
doesChangeAffectWhereClause whereClause oldRow newRow =
    let
        filteredFields =
            extractWhereClauseFields whereClause
    in
    -- Check if any filtered field changed
    Set.foldl
        (\fieldName hasChanged ->
            if hasChanged then
                -- Already found a change, short circuit
                True

            else
                -- Check if this field changed
                Dict.get fieldName oldRow /= Dict.get fieldName newRow
        )
        False
        filteredFields


{-| Determine if a query should be re-executed based on delta changes.

Implementation (Phase 3): Row-level filtering with WHERE clause analysis

  - Check if delta tables overlap with query tables
  - Check if changed row IDs overlap with result row IDs
  - For updates: analyze if filtered fields changed
  - For potential inserts: always re-execute (might match WHERE)

-}
shouldReExecuteQuery : Data.Schema.SchemaMetadata -> Db.Db -> QuerySubscription -> Data.Delta.Delta -> ReExecuteDecision
shouldReExecuteQuery schema db subscription delta =
    let
        -- Get tables used by this query
        queryTables =
            extractQueryTables schema subscription.query

        -- Get changed row IDs grouped by table
        changedRowIds =
            extractChangedRowIds delta

        -- Check each table used by the query
        hasRelevantChanges =
            List.any
                (\tableName ->
                    case ( Dict.get tableName changedRowIds, Dict.get tableName subscription.resultRowIds ) of
                        ( Just deltaIds, Just resultIds ) ->
                            let
                                -- Find rows that are in both delta and result set
                                overlappingIds =
                                    Set.intersect deltaIds resultIds

                                -- If no overlap, check if new rows might match WHERE clause
                                hasNewRows =
                                    not (Set.isEmpty (Set.diff deltaIds resultIds))
                            in
                            if not (Set.isEmpty overlappingIds) then
                                -- Rows in result set changed - need to check WHERE clause
                                analyzeOverlappingChanges schema db subscription tableName overlappingIds delta

                            else if hasNewRows then
                                -- New rows that aren't in result set - only re-execute if they match WHERE
                                checkIfNewRowsMatchWhere schema db tableName (Set.diff deltaIds resultIds) subscription delta

                            else
                                False

                        ( Just _, Nothing ) ->
                            -- Delta has changes but we have no result rows tracked
                            -- This could be first run or empty result set
                            -- Be conservative and re-execute
                            True

                        ( Nothing, _ ) ->
                            -- This table wasn't changed in the delta
                            False
                )
                queryTables
    in
    if hasRelevantChanges then
        ReExecuteFull

    else
        NoReExecute


{-| Analyze if changes to rows already in the result set require re-execution.

This checks if the query has a WHERE clause and if so, whether the changed
fields are referenced in that clause.

Also handles LIMIT/SORT edge cases.

-}
analyzeOverlappingChanges : Data.Schema.SchemaMetadata -> Db.Db -> QuerySubscription -> String -> Set Int -> Data.Delta.Delta -> Bool
analyzeOverlappingChanges schema db subscription tableName overlappingIds delta =
    let
        -- Get the field query for this table
        maybeFieldQuery =
            Dict.foldl
                (\queryFieldName fieldQuery acc ->
                    case acc of
                        Just _ ->
                            acc

                        Nothing ->
                            case Dict.get queryFieldName schema.queryFieldToTable of
                                Just qTableName ->
                                    if qTableName == tableName then
                                        Just fieldQuery

                                    else
                                        Nothing

                                Nothing ->
                                    Nothing
                )
                Nothing
                subscription.query
    in
    case maybeFieldQuery of
        Just fieldQuery ->
            -- Check for LIMIT/SORT - if present, need to be more conservative
            let
                hasLimitOrSort =
                    fieldQuery.limit /= Nothing || fieldQuery.sort /= Nothing
            in
            if hasLimitOrSort then
                -- With LIMIT/SORT, changes might affect ordering or which rows are in top-N
                -- Check if sorted fields changed
                case fieldQuery.sort of
                    Just sortClauses ->
                        let
                            sortedFields =
                                List.map .field sortClauses |> Set.fromList

                            sortedFieldChanged =
                                checkIfSpecificFieldsChanged db tableName overlappingIds sortedFields delta
                        in
                        -- Re-execute if sorted fields changed
                        sortedFieldChanged

                    Nothing ->
                        -- Has LIMIT but no SORT - changes to existing rows still need re-execution
                        True

            else
                -- No LIMIT/SORT - just check WHERE clause
                case fieldQuery.where_ of
                    Just whereClause ->
                        -- Query has WHERE clause - check if filtered fields changed
                        checkIfFilteredFieldsChanged db tableName overlappingIds whereClause delta

                    Nothing ->
                        -- No WHERE clause - any change to result rows requires re-execution
                        True

        Nothing ->
            -- Shouldn't happen, but be conservative
            True


checkIfNewRowsMatchWhere : Data.Schema.SchemaMetadata -> Db.Db -> String -> Set Int -> QuerySubscription -> Data.Delta.Delta -> Bool
checkIfNewRowsMatchWhere schema db tableName newRowIds subscription delta =
    if Set.isEmpty newRowIds then
        False

    else
        let
            maybeFieldQuery =
                Dict.foldl
                    (\queryFieldName fieldQuery acc ->
                        case acc of
                            Just _ ->
                                acc

                            Nothing ->
                                case Dict.get queryFieldName schema.queryFieldToTable of
                                    Just qTableName ->
                                        if qTableName == tableName then
                                            Just fieldQuery

                                        else
                                            Nothing

                                    Nothing ->
                                        Nothing
                    )
                    Nothing
                    subscription.query

            deltaTableGroup =
                List.filter (\tg -> tg.tableName == tableName) delta.tableGroups
                    |> List.head
        in
        case ( maybeFieldQuery, deltaTableGroup ) of
            ( Just fieldQuery, Just tableGroup ) ->
                let
                    whereClause =
                        fieldQuery.where_
                in
                case whereClause of
                    Just _ ->
                        List.any
                            (\newRowArray ->
                                case newRowArray of
                                    (Data.Value.IntValue rowId) :: _ ->
                                        if Set.member rowId newRowIds then
                                            let
                                                newRow =
                                                    rowArrayToDict tableGroup.headers newRowArray
                                            in
                                            Db.rowMatchesWhere whereClause newRow

                                        else
                                            False

                                    _ ->
                                        False
                            )
                            tableGroup.rows

                    Nothing ->
                        True

            _ ->
                True


{-| Check if any of the changed rows had their filtered fields modified.

Gets old row values from DB, new row values from delta, and compares
fields referenced in the WHERE clause.

-}
checkIfFilteredFieldsChanged : Db.Db -> String -> Set Int -> Db.Query.WhereClause -> Data.Delta.Delta -> Bool
checkIfFilteredFieldsChanged db tableName overlappingIds whereClause delta =
    let
        -- Get the table data from DB (old values)
        oldTableData =
            Dict.get tableName db.tables
                |> Maybe.withDefault Dict.empty

        -- Get the new row data from delta
        deltaTableGroup =
            List.filter (\tg -> tg.tableName == tableName) delta.tableGroups
                |> List.head

        -- For each overlapping row, check if filtered fields changed
        anyFilteredFieldChanged =
            case deltaTableGroup of
                Just tableGroup ->
                    List.any
                        (\newRowArray ->
                            case newRowArray of
                                (Data.Value.IntValue rowId) :: _ ->
                                    if Set.member rowId overlappingIds then
                                        -- This row is in both delta and result set
                                        case Dict.get rowId oldTableData of
                                            Just oldRow ->
                                                let
                                                    newRow =
                                                        rowArrayToDict tableGroup.headers newRowArray
                                                in
                                                doesChangeAffectWhereClause whereClause oldRow newRow

                                            Nothing ->
                                                -- Old row not found - conservative, re-execute
                                                True

                                    else
                                        False

                                _ ->
                                    False
                        )
                        tableGroup.rows

                Nothing ->
                    -- Delta table group not found - shouldn't happen
                    False
    in
    anyFilteredFieldChanged


{-| Convert a row array to a dictionary using headers.
-}
rowArrayToDict : List String -> List Value -> Dict String Value
rowArrayToDict headers values =
    List.map2 Tuple.pair headers values
        |> Dict.fromList


{-| Check if specific fields changed in any of the overlapping rows.

Used for SORT field change detection.

-}
checkIfSpecificFieldsChanged : Db.Db -> String -> Set Int -> Set String -> Data.Delta.Delta -> Bool
checkIfSpecificFieldsChanged db tableName overlappingIds fieldsToCheck delta =
    let
        -- Get the table data from DB (old values)
        oldTableData =
            Dict.get tableName db.tables
                |> Maybe.withDefault Dict.empty

        -- Get the new row data from delta
        deltaTableGroup =
            List.filter (\tg -> tg.tableName == tableName) delta.tableGroups
                |> List.head

        -- For each overlapping row, check if specified fields changed
        anyFieldChanged =
            case deltaTableGroup of
                Just tableGroup ->
                    List.any
                        (\newRowArray ->
                            case newRowArray of
                                (Data.Value.IntValue rowId) :: _ ->
                                    if Set.member rowId overlappingIds then
                                        -- This row is in both delta and result set
                                        case Dict.get rowId oldTableData of
                                            Just oldRow ->
                                                let
                                                    newRow =
                                                        rowArrayToDict tableGroup.headers newRowArray
                                                in
                                                -- Check if any of the specified fields changed
                                                Set.foldl
                                                    (\fieldName hasChanged ->
                                                        if hasChanged then
                                                            True

                                                        else
                                                            Dict.get fieldName oldRow /= Dict.get fieldName newRow
                                                    )
                                                    False
                                                    fieldsToCheck

                                            Nothing ->
                                                -- Old row not found - conservative, re-execute
                                                True

                                    else
                                        False

                                _ ->
                                    False
                        )
                        tableGroup.rows

                Nothing ->
                    -- Delta table group not found
                    False
    in
    anyFieldChanged



-- Ports


port queryManagerOut : Encode.Value -> Cmd msg


port receiveQueryManagerMessage : (Decode.Value -> msg) -> Sub msg



-- Encoders


encodeMessage : Message -> Encode.Value
encodeMessage msg =
    case msg of
        QueryResult callbackPort result ->
            Encode.object
                [ ( "type", Encode.string "queryResult" )
                , ( "callbackPort", Encode.string callbackPort )
                , ( "result", result )
                ]

        MutationResult hash result ->
            Encode.object
                [ ( "type", Encode.string "mutationResult" )
                , ( "hash", Encode.string hash )
                , ( "result"
                  , case result of
                        Ok value ->
                            Encode.object
                                [ ( "ok", Encode.bool True )
                                , ( "value", value )
                                ]

                        Err error ->
                            Encode.object
                                [ ( "ok", Encode.bool False )
                                , ( "error", Encode.string error )
                                ]
                  )
                ]



-- Decoders


decodeIncoming : Decode.Decoder Incoming
decodeIncoming =
    Decode.field "type" Decode.string
        |> Decode.andThen
            (\type_ ->
                case type_ of
                    "registerQuery" ->
                        Decode.map4 RegisterQuery
                            (Decode.field "queryId" Decode.string)
                            (Decode.field "query" Db.Query.decodeQuery)
                            (Decode.field "input" Decode.value)
                            (Decode.field "callbackPort" Decode.string)

                    "updateQueryInput" ->
                        Decode.map3 UpdateQueryInput
                            (Decode.field "queryId" Decode.string)
                            (Decode.oneOf
                                [ Decode.field "query" Db.Query.decodeQuery |> Decode.map Just
                                , Decode.succeed Nothing
                                ]
                            )
                            (Decode.field "input" Decode.value)

                    "unregisterQuery" ->
                        Decode.field "queryId" Decode.string
                            |> Decode.map UnregisterQuery

                    "sendMutation" ->
                        Decode.map3 SendMutation
                            (Decode.field "hash" Decode.string)
                            (Decode.field "baseUrl" Decode.string)
                            (Decode.field "input" Decode.value)

                    _ ->
                        Decode.fail ("Unknown QueryManager incoming type: " ++ type_)
            )



-- Helper functions


sendMessage : Message -> Cmd msg
sendMessage msg =
    queryManagerOut (encodeMessage msg)


queryResult : String -> Encode.Value -> Cmd msg
queryResult callbackPort result =
    sendMessage (QueryResult callbackPort result)


mutationResult : String -> Result String Encode.Value -> Cmd msg
mutationResult hash result =
    sendMessage (MutationResult hash result)


receiveIncoming : (Result Decode.Error Incoming -> msg) -> Sub msg
receiveIncoming toMsg =
    receiveQueryManagerMessage (\jsonValue -> toMsg (Decode.decodeValue decodeIncoming jsonValue))



-- Subscriptions


subscriptions : (Incoming -> msg) -> (String -> msg) -> Sub msg
subscriptions toMsg toErrorMsg =
    receiveIncoming
        (\result ->
            case result of
                Ok incoming ->
                    toMsg incoming

                Err err ->
                    toErrorMsg ("Failed to decode QueryManager message: " ++ Decode.errorToString err)
        )
