module Data.Catchup exposing (Model, Msg(..), ServerConfig, Status(..), UpdateResult, databaseId, init, status, update)

import Data.Delta
import Data.IndexedDb
import Data.LiveSync as LiveSync
import Data.Value
import Db
import Dict exposing (Dict)
import Http
import Json.Decode as Decode
import Json.Encode as Encode
import String


type alias ServerConfig =
    { baseUrl : String
    , catchupPath : String
    , databaseId : Maybe String
    , headers : List ( String, String )
    , credentials : String
    , withCredentials : Bool
    }


type Status
    = NotStarted
    | Syncing LiveSync.SyncProgress
    | Synced
    | Error String


type alias SyncCursorEntry =
    { lastSeenUpdatedAt : Maybe Float
    , permissionHash : String
    }


type alias SyncCursor =
    Dict String SyncCursorEntry


type alias CatchupTableResult =
    { rows : List (Dict String Data.Value.Value)
    , permissionHash : String
    , lastSeenUpdatedAt : Maybe Float
    }


type alias CatchupResponse =
    { databaseId : Maybe String
    , serverRevision : Maybe Int
    , tables : Dict String CatchupTableResult
    , hasMore : Bool
    }


type alias Model =
    { server : ServerConfig
    , status : Status
    , cursor : SyncCursor
    , initialDataLoaded : Bool
    , inProgress : Bool
    , tablesSynced : Int
    }


type Msg
    = InitialDataLoaded Data.IndexedDb.SyncCursor
    | CatchupRequired
    | CatchupResponseReceived (Result Http.Error CatchupResponse)


type alias UpdateResult =
    { model : Model
    , db : Db.Db
    , cmd : Cmd Msg
    , dbCmds : List (Cmd Db.Msg)
    , delta : Maybe Data.Delta.Delta
    , serverRevision : Maybe Int
    , touchedTables : List String
    , error : Maybe String
    }


init : ServerConfig -> Model
init server =
    { server = server
    , status = NotStarted
    , cursor = Dict.empty
    , initialDataLoaded = False
    , inProgress = False
    , tablesSynced = 0
    }


status : Model -> Status
status model =
    model.status


databaseId : Model -> Maybe String
databaseId model =
    model.server.databaseId


update : Msg -> Model -> Db.Db -> UpdateResult
update msg model db =
    case msg of
        InitialDataLoaded initialCursor ->
            let
                updatedCursor =
                    computeSyncCursor db initialCursor

                baseModel =
                    { model | cursor = updatedCursor, initialDataLoaded = True }

                ( nextModel, cmd ) =
                    startCatchupIfReady baseModel
            in
            { model = nextModel
            , db = db
            , cmd = cmd
            , dbCmds = [ Data.IndexedDb.writeSyncCursor updatedCursor ]
            , delta = Nothing
            , serverRevision = Nothing
            , touchedTables = []
            , error = Nothing
            }

        CatchupRequired ->
            let
                ( nextModel, cmd ) =
                    startCatchupIfReady model
            in
            { model = nextModel
            , db = db
            , cmd = cmd
            , dbCmds = []
            , delta = Nothing
            , serverRevision = Nothing
            , touchedTables = []
            , error = Nothing
            }

        CatchupResponseReceived result ->
            handleCatchupResponse result model db


startCatchupIfReady : Model -> ( Model, Cmd Msg )
startCatchupIfReady model =
    case ( model.initialDataLoaded, model.inProgress ) of
        ( True, False ) ->
            let
                progress =
                    { table = Nothing
                    , tablesSynced = model.tablesSynced
                    , totalTables = Nothing
                    , complete = False
                    , error = Nothing
                    }
            in
            ( { model | inProgress = True, status = Syncing progress }
            , requestCatchup model.cursor model.server
            )

        _ ->
            ( model, Cmd.none )


handleCatchupResponse : Result Http.Error CatchupResponse -> Model -> Db.Db -> UpdateResult
handleCatchupResponse result model db =
    case result of
        Ok response ->
            case validateResponseDatabaseId model.server.databaseId response.databaseId of
                Just message ->
                    { model = { model | status = Error message, inProgress = False }
                    , db = db
                    , cmd = Cmd.none
                    , dbCmds = []
                    , delta = Nothing
                    , serverRevision = response.serverRevision
                    , touchedTables = []
                    , error = Just message
                    }

                Nothing ->
                    let
                        ( maybeDelta, updatedDb, dbCmds ) =
                            applyCatchupDelta response db

                        updatedCursor =
                            updateSyncCursor response model.cursor

                        syncedCount =
                            model.tablesSynced + Dict.size response.tables

                        progress =
                            { table = Nothing
                            , tablesSynced = syncedCount
                            , totalTables = Nothing
                            , complete = not response.hasMore
                            , error = Nothing
                            }

                        nextStatus =
                            if response.hasMore then
                                Syncing progress

                            else
                                Synced

                        baseModel =
                            { model
                                | cursor = updatedCursor
                                , tablesSynced = syncedCount
                                , status = nextStatus
                                , inProgress = response.hasMore
                            }

                        ( nextModel, cmd ) =
                            if response.hasMore then
                                ( baseModel, requestCatchup updatedCursor model.server )

                            else
                                ( { baseModel | inProgress = False }, Cmd.none )
                    in
                    { model = nextModel
                    , db = updatedDb
                    , cmd = cmd
                    , dbCmds = Data.IndexedDb.writeSyncCursor updatedCursor :: dbCmds
                    , delta = maybeDelta
                    , serverRevision = response.serverRevision
                    , touchedTables = Dict.keys response.tables
                    , error = Nothing
                    }

        Err err ->
            let
                message =
                    httpErrorToString err
            in
            { model = { model | status = Error message, inProgress = False }
            , db = db
            , cmd = Cmd.none
            , dbCmds = []
            , delta = Nothing
            , serverRevision = Nothing
            , touchedTables = []
            , error = Just message
            }


requestCatchup : SyncCursor -> ServerConfig -> Cmd Msg
requestCatchup cursor server =
    let
        url =
            server.baseUrl ++ server.catchupPath

        body =
            Http.jsonBody (encodeCatchupRequest cursor server)
    in
    if includeCredentials server then
        Http.riskyRequest
            { method = "POST"
            , headers = httpHeaders server.headers
            , url = url
            , body = body
            , expect = Http.expectJson CatchupResponseReceived decodeCatchupResponse
            , timeout = Nothing
            , tracker = Nothing
            }

    else
        Http.request
            { method = "POST"
            , headers = httpHeaders server.headers
            , url = url
            , body = body
            , expect = Http.expectJson CatchupResponseReceived decodeCatchupResponse
            , timeout = Nothing
            , tracker = Nothing
            }


encodeCatchupRequest : SyncCursor -> ServerConfig -> Encode.Value
encodeCatchupRequest cursor server =
    Encode.object <|
        List.concat
            [ case server.databaseId of
                Just sourceDatabaseId ->
                    [ ( "databaseId", Encode.string sourceDatabaseId ) ]

                Nothing ->
                    []
            , [ ( "syncCursor", encodeSyncCursor cursor ) ]
            ]


includeCredentials : ServerConfig -> Bool
includeCredentials server =
    server.credentials == "include" || server.withCredentials


httpHeaders : List ( String, String ) -> List Http.Header
httpHeaders headers =
    List.map (\( key, value ) -> Http.header key value) headers


applyCatchupDelta : CatchupResponse -> Db.Db -> ( Maybe Data.Delta.Delta, Db.Db, List (Cmd Db.Msg) )
applyCatchupDelta response db =
    let
        tableGroups =
            response.tables
                |> Dict.toList
                |> List.filterMap
                    (\( tableName, tableResult ) ->
                        catchupTableToGroup tableName tableResult
                    )

        dbWithKnownTables =
            ensureTablesExist (Dict.keys response.tables) db
    in
    if List.isEmpty tableGroups then
        ( Nothing, dbWithKnownTables, [] )

    else
        let
            delta =
                { tableGroups = tableGroups }

            ( updatedDb, dbCmd ) =
                Db.update (Db.DeltaReceived delta) dbWithKnownTables
        in
        ( Just delta, updatedDb, [ dbCmd ] )


ensureTablesExist : List String -> Db.Db -> Db.Db
ensureTablesExist tableNames db =
    let
        updatedTables =
            List.foldl
                (\tableName acc ->
                    case Dict.get tableName acc of
                        Just _ ->
                            acc

                        Nothing ->
                            Dict.insert tableName Dict.empty acc
                )
                db.tables
                tableNames
    in
    { db | tables = updatedTables }


catchupTableToGroup : String -> CatchupTableResult -> Maybe Data.Delta.TableGroup
catchupTableToGroup tableName tableResult =
    case tableResult.rows of
        [] ->
            Nothing

        firstRow :: _ ->
            let
                headers =
                    Dict.keys firstRow

                rows =
                    tableResult.rows
                        |> List.map
                            (\row ->
                                headers
                                    |> List.map
                                        (\header ->
                                            Dict.get header row
                                                |> Maybe.withDefault Data.Value.NullValue
                                        )
                            )
            in
            Just
                { tableName = tableName
                , headers = headers
                , rows = rows
                }


updateSyncCursor : CatchupResponse -> SyncCursor -> SyncCursor
updateSyncCursor response cursor =
    Dict.foldl
        (\tableName tableResult acc ->
            Dict.insert tableName
                { lastSeenUpdatedAt = tableResult.lastSeenUpdatedAt
                , permissionHash = tableResult.permissionHash
                }
                acc
        )
        cursor
        response.tables


computeSyncCursor : Db.Db -> SyncCursor -> SyncCursor
computeSyncCursor db cursor =
    let
        cursorWithMissingTablesReset =
            Dict.foldl
                (\tableName entry acc ->
                    case Dict.get tableName db.tables of
                        Nothing ->
                            resetCursorIfRowsAreMissing tableName entry acc

                        Just tableData ->
                            if Dict.isEmpty tableData then
                                resetCursorIfRowsAreMissing tableName entry acc

                            else
                                acc
                )
                cursor
                cursor
    in
    Dict.foldl
        (\tableName tableData acc ->
            let
                maxUpdatedAt =
                    computeMaxUpdatedAt tableData

                existingPermission =
                    Dict.get tableName cursor
                        |> Maybe.map .permissionHash
                        |> Maybe.withDefault ""

                updatedEntry =
                    { lastSeenUpdatedAt =
                        case maxUpdatedAt of
                            Just _ ->
                                maxUpdatedAt

                            Nothing ->
                                Dict.get tableName cursor
                                    |> Maybe.map .lastSeenUpdatedAt
                                    |> Maybe.withDefault Nothing
                    , permissionHash = existingPermission
                    }
            in
            Dict.insert tableName updatedEntry acc
        )
        cursorWithMissingTablesReset
        db.tables


resetCursorIfRowsAreMissing : String -> SyncCursorEntry -> SyncCursor -> SyncCursor
resetCursorIfRowsAreMissing tableName entry cursor =
    case entry.lastSeenUpdatedAt of
        Just _ ->
            Dict.insert tableName
                { lastSeenUpdatedAt = Nothing
                , permissionHash = ""
                }
                cursor

        Nothing ->
            cursor


computeMaxUpdatedAt : Dict Int (Dict String Data.Value.Value) -> Maybe Float
computeMaxUpdatedAt tableData =
    Dict.values tableData
        |> List.foldl updateMaxUpdatedAt Nothing


updateMaxUpdatedAt : Dict String Data.Value.Value -> Maybe Float -> Maybe Float
updateMaxUpdatedAt row currentMax =
    case Dict.get "updatedAt" row of
        Just value ->
            case valueToTimestamp value of
                Just timestamp ->
                    Just <|
                        case currentMax of
                            Just existing ->
                                max timestamp existing

                            Nothing ->
                                timestamp

                Nothing ->
                    currentMax

        Nothing ->
            currentMax


valueToTimestamp : Data.Value.Value -> Maybe Float
valueToTimestamp value =
    case value of
        Data.Value.IntValue i ->
            Just (toFloat i)

        Data.Value.FloatValue f ->
            Just f

        Data.Value.StringValue str ->
            String.toFloat str

        _ ->
            Nothing


encodeSyncCursor : SyncCursor -> Encode.Value
encodeSyncCursor cursor =
    Encode.object
        [ ( "tables", Encode.dict identity encodeSyncCursorEntry cursor ) ]


encodeSyncCursorEntry : SyncCursorEntry -> Encode.Value
encodeSyncCursorEntry entry =
    Encode.object
        [ ( "last_seen_updated_at"
          , case entry.lastSeenUpdatedAt of
                Just value ->
                    Encode.float value

                Nothing ->
                    Encode.null
          )
        , ( "permission_hash", Encode.string entry.permissionHash )
        ]


decodeCatchupResponse : Decode.Decoder CatchupResponse
decodeCatchupResponse =
    Decode.map4 CatchupResponse
        (Decode.maybe (Decode.field "databaseId" Decode.string))
        (Decode.maybe (Decode.field "serverRevision" Decode.int))
        (Decode.field "tables" (Decode.dict decodeCatchupTable))
        (Decode.field "has_more" Decode.bool)


validateResponseDatabaseId : Maybe String -> Maybe String -> Maybe String
validateResponseDatabaseId expected actual =
    case expected of
        Nothing ->
            Nothing

        Just expectedId ->
            case actual of
                Just actualId ->
                    if actualId == expectedId then
                        Nothing

                    else
                        Just ("Catchup response databaseId mismatch: expected " ++ expectedId ++ ", got " ++ actualId)

                Nothing ->
                    Just ("Catchup response missing databaseId: expected " ++ expectedId)


decodeCatchupTable : Decode.Decoder CatchupTableResult
decodeCatchupTable =
    Decode.map3 CatchupTableResult
        (Decode.field "rows" (Decode.list (Decode.dict Data.Value.decodeValue)))
        (Decode.field "permission_hash" Decode.string)
        (Decode.field "last_seen_updated_at" decodeMaybeTimestamp)


decodeMaybeTimestamp : Decode.Decoder (Maybe Float)
decodeMaybeTimestamp =
    Decode.oneOf
        [ Decode.null Nothing
        , Decode.float |> Decode.map Just
        , Decode.int |> Decode.map (\value -> Just (toFloat value))
        ]


httpErrorToString : Http.Error -> String
httpErrorToString error =
    case error of
        Http.BadUrl url ->
            "Bad URL: " ++ url

        Http.Timeout ->
            "Timeout"

        Http.NetworkError ->
            "Network Error"

        Http.BadStatus code ->
            "Bad Status: " ++ String.fromInt code

        Http.BadBody message ->
            "Decode Error: " ++ message
