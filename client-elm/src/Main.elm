module Main exposing (main)

import Data.Error
import Data.IndexedDb as IndexedDb exposing (Incoming(..))
import Data.QueryManager as QueryManager exposing (Incoming(..), Msg(..))
import Data.SSE as SSE exposing (Incoming(..))
import Data.Schema
import Data.Value
import Db exposing (Msg(..))
import Db.Query
import Dict exposing (Dict)
import Http
import Json.Decode as Decode
import Json.Encode as Encode
import Platform



-- Flags


type alias Flags =
    { schema : Data.Schema.SchemaMetadata
    , sseConfig : SSE.SSEConfig
    }



-- Model


type alias Model =
    { schema : Data.Schema.SchemaMetadata
    , db : Db.Db
    , queryManager : QueryManager.Model
    , sseConfig : SSE.SSEConfig
    , syncStatus : SyncStatus
    }


type SyncStatus
    = NotStarted
    | Syncing SSE.SyncProgress
    | Synced
    | SyncError String



-- Msg


type Msg
    = IndexedDbReceived IndexedDb.Incoming
    | SSEReceived SSE.Incoming
    | QueryManagerReceived QueryManager.Incoming
    | MutationRequest String String Encode.Value (Result Http.Error Encode.Value)
    | DbMsg Db.Msg
    | Error String



-- Init


init : Flags -> ( Model, Cmd Msg )
init flags =
    ( { schema = flags.schema
      , db = Db.init
      , queryManager = QueryManager.init
      , sseConfig = flags.sseConfig
      , syncStatus = NotStarted
      }
    , Cmd.batch
        [ IndexedDb.requestInitialData
        , SSE.connect flags.sseConfig
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
            in
            ( { model | db = updatedDb }
            , Cmd.batch
                [ Cmd.map DbMsg dbCmd
                , handleIndexedDbIncoming incoming model
                ]
            )

        SSEReceived incoming ->
            handleSSEIncoming incoming model

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

        DbMsg dbMsg ->
            let
                ( updatedDb, dbCmd ) =
                    Db.update dbMsg model.db
            in
            ( { model | db = updatedDb }
            , Cmd.map DbMsg dbCmd
            )


handleIndexedDbIncoming : IndexedDb.Incoming -> Model -> Cmd Msg
handleIndexedDbIncoming incoming model =
    case incoming of
        IndexedDb.InitialDataReceived _ ->
            -- Database already updated via Db.update
            Cmd.none


handleSSEIncoming : SSE.Incoming -> Model -> ( Model, Cmd Msg )
handleSSEIncoming incoming model =
    case incoming of
        SSE.DeltaReceived delta ->
            -- Update database with delta
            let
                ( updatedDb, dbCmd ) =
                    Db.update (Db.DeltaReceived model.schema delta) model.db

                -- Notify query manager of affected tables
                affectedTables =
                    Db.extractAffectedTables delta

                triggerCmds =
                    QueryManager.notifyTablesChanged model.schema updatedDb model.queryManager affectedTables
            in
            ( { model | db = updatedDb }
            , Cmd.batch
                [ Cmd.map DbMsg dbCmd
                , Cmd.batch triggerCmds
                ]
            )

        SSE.SSEConnected _ ->
            ( model, Cmd.none )

        SSE.SSEError error ->
            ( { model | syncStatus = SyncError error }
            , Cmd.none
            )

        SSE.SyncProgressReceived progress ->
            ( { model | syncStatus = Syncing progress }
            , Cmd.none
            )

        SSE.SyncCompleteReceived ->
            ( { model | syncStatus = Synced }
            , Cmd.none
            )


handleQueryManagerIncoming : QueryManager.Incoming -> Model -> ( Model, List (Cmd Msg) )
handleQueryManagerIncoming incoming model =
    case incoming of
        QueryManager.RegisterQuery queryId query input callbackPort ->
            -- QueryManager already updated the subscription
            -- Execute query and send result
            let
                result =
                    Db.executeQuery model.schema model.db query

                resultJson =
                    encodeQueryResult result
            in
            ( model
            , [ QueryManager.queryResult callbackPort resultJson ]
            )

        QueryManager.UpdateQueryInput queryId newInput ->
            -- QueryManager already updated the subscription
            -- Re-execute query and send result
            case Dict.get queryId model.queryManager.subscriptions of
                Just subscription ->
                    let
                        result =
                            Db.executeQuery model.schema model.db subscription.query

                        resultJson =
                            encodeQueryResult result
                    in
                    ( model
                    , [ QueryManager.queryResult subscription.callbackPort resultJson ]
                    )

                Nothing ->
                    ( model, [] )

        QueryManager.UnregisterQuery _ ->
            ( model, [] )

        QueryManager.SendMutation hash baseUrl input ->
            -- Mutations are handled via HTTP request
            ( model
            , [ Http.post
                    { url = baseUrl ++ "/" ++ hash
                    , body = Http.jsonBody input
                    , expect =
                        Http.expectStringResponse
                            (MutationRequest hash baseUrl input)
                            (\response ->
                                case response of
                                    Http.BadUrl_ url ->
                                        Err (Http.BadUrl url)

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
        , SSE.receiveIncoming
            (\result ->
                case result of
                    Ok incoming ->
                        SSEReceived incoming

                    Err err ->
                        -- Send error to console
                        Error ("Failed to decode SSE message: " ++ Decode.errorToString err)
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
                        -- Fallback with empty schema and default SSE config
                        init
                            { schema =
                                { tables = Dict.empty
                                , queryFieldToTable = Dict.empty
                                }
                            , sseConfig =
                                { baseUrl = ""
                                , userId = 0
                                }
                            }
        , update = update
        , subscriptions = subscriptions
        }


decodeFlags : Decode.Decoder Flags
decodeFlags =
    Decode.map2 Flags
        (Decode.field "schema" Data.Schema.decodeSchemaMetadata)
        (Decode.field "sseConfig" SSE.decodeSSEConfig)
