module Data.Catchup exposing (Model, Msg(..), ServerConfig, Status(..), UpdateResult, init, status, update)

import Data.Delta
import Data.LiveSync as LiveSync
import Data.Value
import Db
import Dict exposing (Dict)
import Http
import Json.Decode as Decode
import Json.Encode as Encode
import String
import Url


type alias ServerConfig =
    { baseUrl : String
    , catchupPath : String
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
    { tables : Dict String CatchupTableResult
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
    = InitialDataLoaded
    | CatchupResponseReceived (Result Http.Error CatchupResponse)


type alias UpdateResult =
    { model : Model
    , db : Db.Db
    , cmd : Cmd Msg
    , dbCmds : List (Cmd Db.Msg)
    , delta : Maybe Data.Delta.Delta
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


update : Msg -> Model -> Db.Db -> UpdateResult
update msg model db =
    case msg of
        InitialDataLoaded ->
            let
                updatedCursor =
                    computeSyncCursor db model.cursor

                baseModel =
                    { model | cursor = updatedCursor, initialDataLoaded = True }

                ( nextModel, cmd ) =
                    startCatchupIfReady baseModel
            in
            { model = nextModel
            , db = db
            , cmd = cmd
            , dbCmds = []
            , delta = Nothing
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
            , dbCmds = dbCmds
            , delta = maybeDelta
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
            , error = Just message
            }


requestCatchup : SyncCursor -> ServerConfig -> Cmd Msg
requestCatchup cursor server =
    let
        params =
            if Dict.isEmpty cursor then
                []

            else
                [ ( "syncCursor", Encode.encode 0 (encodeSyncCursor cursor) ) ]

        url =
            appendQueryParams (server.baseUrl ++ server.catchupPath) params
    in
    Http.get
        { url = url
        , expect = Http.expectJson CatchupResponseReceived decodeCatchupResponse
        }


applyCatchupDelta : CatchupResponse -> Db.Db -> ( Maybe Data.Delta.Delta, Db.Db, List (Cmd Db.Msg) )
applyCatchupDelta response db =
    let
        tableGroups =
            response.tables
                |> Dict.toList
                |> List.filterMap (\( tableName, tableResult ) ->
                    catchupTableToGroup tableName tableResult
                   )
    in
    if List.isEmpty tableGroups then
        ( Nothing, db, [] )

    else
        let
            delta =
                { tableGroups = tableGroups }

            ( updatedDb, dbCmd ) =
                Db.update (Db.DeltaReceived delta) db
        in
        ( Just delta, updatedDb, [ dbCmd ] )


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
                        |> List.map (\row ->
                            headers
                                |> List.map (\header ->
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
        cursor
        db.tables


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
    Decode.map2 CatchupResponse
        (Decode.field "tables" (Decode.dict decodeCatchupTable))
        (Decode.field "has_more" Decode.bool)


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


appendQueryParams : String -> List ( String, String ) -> String
appendQueryParams url params =
    case String.split "?" url of
        base :: queryParts ->
            let
                existing =
                    String.join "?" queryParts

                encodedParams =
                    params
                        |> List.map (\( key, value ) -> key ++ "=" ++ Url.percentEncode value)

                combined =
                    List.filter (\item -> not (String.isEmpty item)) (existing :: encodedParams)
                        |> String.join "&"
            in
            if String.isEmpty combined then
                base
            else
                base ++ "?" ++ combined

        [] ->
            url


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
