port module Main exposing (main)

import Data.Catchup as Catchup
import Data.Error
import Data.IndexedDb as IndexedDb exposing (Incoming(..))
import Data.LiveSync as LiveSync exposing (Incoming(..))
import Data.QueryManager as QueryManager exposing (Incoming(..), Msg(..))
import Data.Schema
import Data.SyncState as SyncState
import Data.Value
import Db exposing (Msg(..))
import Db.Query
import Dict exposing (Dict)
import Http
import Json.Decode as Decode
import Json.Encode as Encode
import Platform
import String



-- Flags


type alias Flags =
    { schema : Data.Schema.SchemaMetadata
    , server : Catchup.ServerConfig
    , liveSync : LiveSync.Config
    }



-- Model


type alias Model =
    { schema : Data.Schema.SchemaMetadata
    , db : Db.Db
    , queryManager : QueryManager.Model
    , catchup : Catchup.Model
    , syncStatus : SyncState.SyncStatus
    , tableSyncStatuses : Dict String SyncState.TableSyncStatus
    , syncError : Maybe String
    , liveSyncStarted : Bool
    , liveSyncTransport : LiveSync.Transport
    }



-- Msg


type Msg
    = IndexedDbReceived IndexedDb.Incoming
    | LiveSyncReceived LiveSync.Incoming
    | QueryManagerReceived QueryManager.Incoming
    | QueryClientReceived QueryManager.QueryClientIncoming
    | MutationRequest String String Encode.Value (Result Http.Error Encode.Value)
    | DbMsg Db.Msg
    | Error String
    | CatchupMsg Catchup.Msg



-- Init


init : Flags -> ( Model, Cmd Msg )
init flags =
    ( { schema = flags.schema
      , db = Db.init
      , queryManager = QueryManager.init
      , catchup = Catchup.init flags.server
      , syncStatus = SyncState.NotStarted
      , tableSyncStatuses = SyncState.initialTableStatuses flags.schema.tables
      , syncError = Nothing
      , liveSyncStarted = False
      , liveSyncTransport = flags.liveSync.transport
      }
    , Cmd.batch
        [ IndexedDb.requestInitialData
        , emitSyncState
            { status = SyncState.NotStarted
            , tables = SyncState.initialTableStatuses flags.schema.tables
            }
        ]
    )



-- Update


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        IndexedDbReceived incoming ->
            let
                ( updatedDb, dbCmd ) =
                    Db.update (Db.FromIndexedDb model.schema incoming) model.db

                baseModel =
                    { model | db = updatedDb }

                ( updatedModel, indexedDbCmd ) =
                    handleIndexedDbIncoming incoming baseModel
            in
            ( updatedModel
            , Cmd.batch
                [ Cmd.map DbMsg dbCmd
                , indexedDbCmd
                ]
            )

        LiveSyncReceived incoming ->
            handleLiveSyncIncoming incoming model

        QueryManagerReceived incoming ->
            let
                ( updatedQueryManager, _ ) =
                    QueryManager.update (QueryManager.IncomingReceived incoming) model.queryManager

                ( updatedModel, queryCmds ) =
                    handleQueryManagerIncoming incoming { model | queryManager = updatedQueryManager }
            in
            ( updatedModel
            , Cmd.batch queryCmds
            )

        QueryClientReceived incoming ->
            let
                ( updatedModel, queryCmds ) =
                    handleQueryClientIncoming incoming model
            in
            ( updatedModel
            , Cmd.batch queryCmds
            )

        MutationRequest id baseUrl input result ->
            case result of
                Ok response ->
                    ( model
                    , QueryManager.mutationResult id (Ok response)
                    )

                Err error ->
                    ( model
                    , QueryManager.mutationResult id (Err (httpErrorToString error))
                    )

        Error errorMessage ->
            ( model
            , Data.Error.sendError errorMessage
            )

        CatchupMsg catchupMsg ->
            applyCatchupUpdate (Catchup.update catchupMsg model.catchup model.db) model

        DbMsg dbMsg ->
            let
                ( updatedDb, dbCmd ) =
                    Db.update dbMsg model.db
            in
            ( { model | db = updatedDb }
            , Cmd.map DbMsg dbCmd
            )


handleIndexedDbIncoming : IndexedDb.Incoming -> Model -> ( Model, Cmd Msg )
handleIndexedDbIncoming incoming model =
    case incoming of
        IndexedDb.InitialDataReceived _ ->
            let
                ( updatedQueryManager, cmds ) =
                    reExecuteAllQueries model.schema model.db model.queryManager

                baseModel =
                    { model | queryManager = updatedQueryManager }

                ( catchupModel, catchupCmd ) =
                    applyCatchupUpdate (Catchup.update Catchup.InitialDataLoaded model.catchup model.db) baseModel
            in
            ( catchupModel
            , Cmd.batch [ Cmd.batch cmds, catchupCmd ]
            )


handleLiveSyncIncoming : LiveSync.Incoming -> Model -> ( Model, Cmd Msg )
handleLiveSyncIncoming incoming model =
    case incoming of
        LiveSync.DeltaReceived delta ->
            -- Update database with delta
            let
                ( updatedDb, dbCmd ) =
                    Db.update (Db.DeltaReceived delta) model.db

                -- Notify query manager with fine-grained reactivity
                ( updatedQueryManager, triggerCmds ) =
                    QueryManager.notifyTablesChanged model.schema updatedDb model.queryManager delta
            in
            ( { model | db = updatedDb, queryManager = updatedQueryManager }
            , Cmd.batch
                [ Cmd.map DbMsg dbCmd
                , Cmd.batch triggerCmds
                ]
            )

        LiveSync.LiveSyncConnected _ ->
            ( model, Cmd.none )

        LiveSync.LiveSyncError error ->
            ( { model | syncError = Just error }
            , Cmd.batch
                [ emitSyncState (toSyncState model)
                , Data.Error.sendError error
                ]
            )

        LiveSync.SyncProgressReceived _ ->
            let
                updatedModel =
                    { model | syncStatus = SyncState.CatchingUp }
            in
            ( updatedModel
            , emitSyncState (toSyncState updatedModel)
            )

        LiveSync.SyncCompleteReceived ->
            let
                updatedModel =
                    { model
                        | syncStatus = SyncState.Live
                        , tableSyncStatuses = SyncState.markAllTablesLive model.tableSyncStatuses
                        , syncError = Nothing
                    }
            in
            ( updatedModel
            , emitSyncState (toSyncState updatedModel)
            )


handleQueryManagerIncoming : QueryManager.Incoming -> Model -> ( Model, List (Cmd Msg) )
handleQueryManagerIncoming incoming model =
    case incoming of
        QueryManager.SendMutation id baseUrl headers input ->
            -- Mutations are handled via HTTP request
            let
                url =
                    buildMutationUrl baseUrl id
            in
            ( model
            , [ Http.request
                    { method = "POST"
                    , headers = List.map (\( key, value ) -> Http.header key value) headers
                    , url = url
                    , body = Http.jsonBody input
                    , expect =
                        Http.expectStringResponse
                            (MutationRequest id baseUrl input)
                            (\response ->
                                case response of
                                    Http.BadUrl_ badUrl ->
                                        Err (Http.BadUrl badUrl)

                                    Http.Timeout_ ->
                                        Err Http.Timeout

                                    Http.NetworkError_ ->
                                        Err Http.NetworkError

                                    Http.BadStatus_ metadata body ->
                                        Err (Http.BadStatus metadata.statusCode)

                                    Http.GoodStatus_ _ body ->
                                        case Decode.decodeString Decode.value body of
                                            Ok json ->
                                                Ok json

                                            Err err ->
                                                Err (Http.BadBody (Decode.errorToString err))
                            )
                    , timeout = Nothing
                    , tracker = Nothing
                    }
              ]
            )


handleQueryClientIncoming : QueryManager.QueryClientIncoming -> Model -> ( Model, List (Cmd Msg) )
handleQueryClientIncoming incoming model =
    case incoming of
        QueryManager.QCRegister queryId query input ->
            -- Register the query and execute it immediately
            let
                subscription =
                    QueryManager.QuerySubscription queryId query input "" Dict.empty 0 Nothing

                updatedSubscriptions =
                    Dict.insert queryId subscription model.queryManager.subscriptions

                executionResult =
                    Db.executeQueryWithTracking model.schema model.db query

                resultJson =
                    encodeQueryResult executionResult.results

                nextRevision =
                    1

                finalSubscription =
                    { subscription
                        | resultRowIds = executionResult.rowIds
                        , revision = nextRevision
                        , lastResult = Just executionResult.results
                    }

                finalSubscriptions =
                    Dict.insert queryId finalSubscription updatedSubscriptions

                updatedQueryManager =
                    { subscriptions = finalSubscriptions }
            in
            ( { model | queryManager = updatedQueryManager }
            , [ QueryManager.queryClientFull queryId nextRevision resultJson ]
            )

        QueryManager.QCUpdateInput queryId maybeQuery newInput ->
            -- Update the input and re-execute
            case Dict.get queryId model.queryManager.subscriptions of
                Just subscription ->
                    let
                        nextQuery =
                            Maybe.withDefault subscription.query maybeQuery

                        updatedSubscription =
                            { subscription
                                | query = nextQuery
                                , input = newInput
                                , resultRowIds = Dict.empty
                                , lastResult = Nothing
                            }

                        executionResult =
                            Db.executeQueryWithTracking model.schema model.db nextQuery

                        resultJson =
                            encodeQueryResult executionResult.results

                        nextRevision =
                            subscription.revision + 1

                        finalSubscription =
                            { updatedSubscription
                                | resultRowIds = executionResult.rowIds
                                , revision = nextRevision
                                , lastResult = Just executionResult.results
                            }

                        updatedSubscriptions =
                            Dict.insert queryId finalSubscription model.queryManager.subscriptions

                        updatedQueryManager =
                            { subscriptions = updatedSubscriptions }
                    in
                    ( { model | queryManager = updatedQueryManager }
                    , [ QueryManager.queryClientFull queryId nextRevision resultJson ]
                    )

                Nothing ->
                    ( model, [] )

        QueryManager.QCUnregister queryId ->
            let
                updatedSubscriptions =
                    Dict.remove queryId model.queryManager.subscriptions

                updatedQueryManager =
                    { subscriptions = updatedSubscriptions }
            in
            ( { model | queryManager = updatedQueryManager }
            , []
            )



-- Helper Functions


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


buildMutationUrl : String -> String -> String
buildMutationUrl baseUrl id =
    case String.split "?" baseUrl of
        base :: queryParts ->
            let
                query =
                    String.join "?" queryParts
            in
            if String.isEmpty query then
                base ++ "/" ++ id

            else
                base ++ "/" ++ id ++ "?" ++ query

        [] ->
            baseUrl ++ "/" ++ id


applyCatchupUpdate : Catchup.UpdateResult -> Model -> ( Model, Cmd Msg )
applyCatchupUpdate result model =
    let
        nextSyncStatus =
            syncStatusFromCatchup result.model

        nextTableSyncStatuses =
            case nextSyncStatus of
                SyncState.NotStarted ->
                    model.tableSyncStatuses

                SyncState.CatchingUp ->
                    SyncState.markTablesCatchingUp result.touchedTables model.tableSyncStatuses

                SyncState.Live ->
                    SyncState.markAllTablesLive model.tableSyncStatuses

        updatedModel =
            { model
                | catchup = result.model
                , db = result.db
                , syncStatus = nextSyncStatus
                , tableSyncStatuses = nextTableSyncStatuses
                , syncError =
                    case result.error of
                        Just message ->
                            Just message

                        Nothing ->
                            if nextSyncStatus == SyncState.Live then
                                Nothing

                            else
                                model.syncError
            }

        ( liveSyncModel, liveSyncCmd ) =
            startLiveSyncIfReady updatedModel

        ( updatedQueryManager, triggerCmds ) =
            case result.delta of
                Just delta ->
                    QueryManager.notifyTablesChanged model.schema result.db model.queryManager delta

                Nothing ->
                    ( model.queryManager, [] )

        errorCmd =
            case result.error of
                Just message ->
                    Data.Error.sendError message

                Nothing ->
                    Cmd.none

        dbCmds =
            result.dbCmds
                |> List.map (Cmd.map DbMsg)

        cmds =
            [ Cmd.map CatchupMsg result.cmd
            , errorCmd
            , Cmd.batch triggerCmds
            , liveSyncCmd
            , emitSyncState (toSyncState liveSyncModel)
            ]
                ++ dbCmds
    in
    ( { liveSyncModel | queryManager = updatedQueryManager }
    , Cmd.batch cmds
    )


syncStatusFromCatchup : Catchup.Model -> SyncState.SyncStatus
syncStatusFromCatchup catchupModel =
    case Catchup.status catchupModel of
        Catchup.NotStarted ->
            SyncState.NotStarted

        Catchup.Syncing _ ->
            SyncState.CatchingUp

        Catchup.Synced ->
            SyncState.Live

        Catchup.Error _ ->
            SyncState.CatchingUp


toSyncState : Model -> SyncState.SyncState
toSyncState model =
    { status = model.syncStatus
    , tables = model.tableSyncStatuses
    }


emitSyncState : SyncState.SyncState -> Cmd Msg
emitSyncState syncState =
    syncStateOut (SyncState.encodeSyncState syncState)


port syncStateOut : Encode.Value -> Cmd msg


startLiveSyncIfReady : Model -> ( Model, Cmd Msg )
startLiveSyncIfReady model =
    case ( model.liveSyncStarted, Catchup.status model.catchup ) of
        ( False, Catchup.Synced ) ->
            ( { model | liveSyncStarted = True }
            , LiveSync.connect
                { transport = model.liveSyncTransport }
            )

        ( False, Catchup.Error _ ) ->
            ( { model | liveSyncStarted = True }
            , LiveSync.connect
                { transport = model.liveSyncTransport }
            )

        _ ->
            ( model, Cmd.none )


encodeQueryResult : Dict String (List (Dict String Data.Value.Value)) -> Encode.Value
encodeQueryResult result =
    Encode.dict identity
        (\rows ->
            Encode.list (\row -> Encode.dict identity Data.Value.encodeValue row) rows
        )
        result



-- Subscriptions


subscriptions : Model -> Sub Msg
subscriptions model =
    Sub.batch
        [ IndexedDb.receiveIncoming
            (\result ->
                case result of
                    Ok incoming ->
                        IndexedDbReceived incoming

                    Err err ->
                        -- Send error to console
                        Error ("Failed to decode IndexedDB message: " ++ Decode.errorToString err)
            )
        , LiveSync.receiveIncoming
            (\result ->
                case result of
                    Ok incoming ->
                        LiveSyncReceived incoming

                    Err err ->
                        -- Send error to console
                        Error ("Failed to decode LiveSync message: " ++ Decode.errorToString err)
            )
        , QueryManager.receiveIncoming
            (\result ->
                case result of
                    Ok incoming ->
                        QueryManagerReceived incoming

                    Err err ->
                        -- Send error to console
                        Error ("Failed to decode QueryManager message: " ++ Decode.errorToString err)
            )
        , QueryManager.receiveQueryClientIncoming
            (\result ->
                case result of
                    Ok incoming ->
                        QueryClientReceived incoming

                    Err err ->
                        -- Send error to console
                        Error ("Failed to decode QueryClient message: " ++ Decode.errorToString err)
            )
        ]



-- Main


main : Program Decode.Value Model Msg
main =
    Platform.worker
        { init =
            \flagsJson ->
                case Decode.decodeValue decodeFlags flagsJson of
                    Ok flags ->
                        init flags

                    Err err ->
                        -- Fallback with empty schema and default live sync config
                        init
                            { schema =
                                { tables = Dict.empty
                                , queryFieldToTable = Dict.empty
                                }
                            , server =
                                { baseUrl = ""
                                , catchupPath = ""
                                }
                            , liveSync =
                                { transport = LiveSync.Sse }
                            }
        , update = update
        , subscriptions = subscriptions
        }


decodeFlags : Decode.Decoder Flags
decodeFlags =
    Decode.map3 Flags
        (Decode.field "schema" Data.Schema.decodeSchemaMetadata)
        (Decode.field "server" decodeServerConfig)
        (Decode.oneOf
            [ Decode.field "liveSync" LiveSync.decodeConfig
            , Decode.succeed { transport = LiveSync.Sse }
            ]
        )


decodeServerConfig : Decode.Decoder Catchup.ServerConfig
decodeServerConfig =
    Decode.map2 Catchup.ServerConfig
        (Decode.field "baseUrl" Decode.string)
        (Decode.field "catchupPath" Decode.string)


reExecuteAllQueries : Data.Schema.SchemaMetadata -> Db.Db -> QueryManager.Model -> ( QueryManager.Model, List (Cmd Msg) )
reExecuteAllQueries schema db queryManager =
    Dict.foldl
        (\_ subscription ( accModel, accCmds ) ->
            let
                executionResult =
                    Db.executeQueryWithTracking schema db subscription.query

                resultJson =
                    encodeQueryResult executionResult.results

                nextRevision =
                    subscription.revision + 1

                updatedSubscription =
                    { subscription
                        | resultRowIds = executionResult.rowIds
                        , revision = nextRevision
                        , lastResult = Just executionResult.results
                    }

                updatedSubscriptions =
                    Dict.insert subscription.queryId updatedSubscription accModel.subscriptions

                updatedModel =
                    { accModel | subscriptions = updatedSubscriptions }
            in
            ( updatedModel
            , QueryManager.queryClientFull subscription.queryId nextRevision resultJson :: accCmds
            )
        )
        ( queryManager, [] )
        queryManager.subscriptions
