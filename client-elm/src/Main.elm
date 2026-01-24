module Main exposing (main)

import Data.Catchup as Catchup
import Data.Error
import Data.IndexedDb as IndexedDb exposing (Incoming(..))
import Data.QueryManager as QueryManager exposing (Incoming(..), Msg(..))
import Data.LiveSync as LiveSync exposing (Incoming(..))
import Data.Schema
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
    , syncStatus : SyncStatus
    , liveSyncStarted : Bool
    , liveSyncTransport : LiveSync.Transport
    }


type SyncStatus
    = NotStarted
    | Syncing LiveSync.SyncProgress
    | Synced
    | SyncError String



-- Msg


type Msg
    = IndexedDbReceived IndexedDb.Incoming
    | LiveSyncReceived LiveSync.Incoming
    | QueryManagerReceived QueryManager.Incoming
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
      , syncStatus = NotStarted
      , liveSyncStarted = False
      , liveSyncTransport = flags.liveSync.transport
      }
    , Cmd.batch
        [ IndexedDb.requestInitialData ]
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

        MutationRequest hash baseUrl input result ->
            case result of
                Ok response ->
                    ( model
                    , QueryManager.mutationResult hash (Ok response)
                    )

                Err error ->
                    ( model
                    , QueryManager.mutationResult hash (Err (httpErrorToString error))
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
            ( { model | syncStatus = SyncError error }
            , Cmd.none
            )

        LiveSync.SyncProgressReceived progress ->
            ( { model | syncStatus = Syncing progress }
            , Cmd.none
            )

        LiveSync.SyncCompleteReceived ->
            ( { model | syncStatus = Synced }
            , Cmd.none
            )


handleQueryManagerIncoming : QueryManager.Incoming -> Model -> ( Model, List (Cmd Msg) )
handleQueryManagerIncoming incoming model =
    case incoming of
        QueryManager.RegisterQuery queryId query input callbackPort ->
            -- QueryManager already updated the subscription
            -- Execute query and send result, and update subscription with row IDs
            let
                executionResult =
                    Db.executeQueryWithTracking model.schema model.db query

                resultJson =
                    encodeQueryResult executionResult.results

                -- Update subscription with row IDs
                updatedQueryManager =
                    case Dict.get queryId model.queryManager.subscriptions of
                        Just subscription ->
                            let
                                updatedSubscription =
                                    { subscription | resultRowIds = executionResult.rowIds }

                                updatedSubscriptions =
                                    Dict.insert queryId updatedSubscription model.queryManager.subscriptions
                            in
                            { subscriptions = updatedSubscriptions }

                        Nothing ->
                            model.queryManager
            in
            ( { model | queryManager = updatedQueryManager }
            , [ QueryManager.queryResult callbackPort resultJson ]
            )

        QueryManager.UpdateQueryInput queryId _ newInput ->
            -- QueryManager already updated the subscription
            -- Re-execute query and send result, and update subscription with row IDs
            case Dict.get queryId model.queryManager.subscriptions of
                Just subscription ->
                    let
                        executionResult =
                            Db.executeQueryWithTracking model.schema model.db subscription.query

                        resultJson =
                            encodeQueryResult executionResult.results

                        -- Update subscription with row IDs
                        updatedSubscription =
                            { subscription | resultRowIds = executionResult.rowIds }

                        updatedSubscriptions =
                            Dict.insert queryId updatedSubscription model.queryManager.subscriptions

                        updatedQueryManager =
                            { subscriptions = updatedSubscriptions }
                    in
                    ( { model | queryManager = updatedQueryManager }
                    , [ QueryManager.queryResult subscription.callbackPort resultJson ]
                    )

                Nothing ->
                    ( model, [] )

        QueryManager.UnregisterQuery _ ->
            ( model, [] )

        QueryManager.SendMutation hash baseUrl headers input ->
            -- Mutations are handled via HTTP request
            let
                url =
                    buildMutationUrl baseUrl hash
            in
            ( model
            , [ Http.request
                    { method = "POST"
                    , headers = List.map (\( key, value ) -> Http.header key value) headers
                    , url = url
                    , body = Http.jsonBody input
                    , expect =
                        Http.expectStringResponse
                            (MutationRequest hash baseUrl input)
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
buildMutationUrl baseUrl hash =
    case String.split "?" baseUrl of
        base :: queryParts ->
            let
                query =
                    String.join "?" queryParts
            in
            if String.isEmpty query then
                base ++ "/" ++ hash

            else
                base ++ "/" ++ hash ++ "?" ++ query

        [] ->
            baseUrl ++ "/" ++ hash


applyCatchupUpdate : Catchup.UpdateResult -> Model -> ( Model, Cmd Msg )
applyCatchupUpdate result model =
    let
        updatedModel =
            { model
                | catchup = result.model
                , db = result.db
                , syncStatus = syncStatusFromCatchup result.model
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
            ]
                ++ dbCmds
    in
    ( { liveSyncModel | queryManager = updatedQueryManager }
    , Cmd.batch cmds
    )


syncStatusFromCatchup : Catchup.Model -> SyncStatus
syncStatusFromCatchup catchupModel =
    case Catchup.status catchupModel of
        Catchup.NotStarted ->
            NotStarted

        Catchup.Syncing progress ->
            Syncing progress

        Catchup.Synced ->
            Synced

        Catchup.Error message ->
            SyncError message


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

                updatedSubscription =
                    { subscription | resultRowIds = executionResult.rowIds }

                updatedSubscriptions =
                    Dict.insert subscription.queryId updatedSubscription accModel.subscriptions

                updatedModel =
                    { accModel | subscriptions = updatedSubscriptions }
            in
            ( updatedModel
            , QueryManager.queryResult subscription.callbackPort resultJson :: accCmds
            )
        )
        ( queryManager, [] )
        queryManager.subscriptions
