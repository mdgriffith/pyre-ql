port module Data.QueryManager exposing (Incoming(..), Message, Model, Msg(..), QuerySubscription, decodeIncoming, encodeMessage, init, mutationResult, notifyTablesChanged, queryResult, receiveIncoming, sendMessage, subscriptions, update)

import Data.Schema
import Data.Value exposing (Value)
import Db
import Db.Query
import Dict exposing (Dict)
import Json.Decode as Decode
import Json.Encode as Encode



-- Model


type alias Model =
    { subscriptions : Dict String QuerySubscription
    }


type alias QuerySubscription =
    { queryId : String
    , query : Db.Query.Query
    , input : Encode.Value
    , callbackPort : String
    }



-- Messages


type Msg
    = IncomingReceived Incoming


type Incoming
    = RegisterQuery String Db.Query.Query Encode.Value String -- queryId, query, input, callbackPort
    | UpdateQueryInput String Encode.Value -- queryId, newInput
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
                    QuerySubscription queryId query input callbackPort

                updatedSubscriptions =
                    Dict.insert queryId subscription model.subscriptions
            in
            ( { model | subscriptions = updatedSubscriptions }
            , Cmd.none
            )

        UpdateQueryInput queryId newInput ->
            case Dict.get queryId model.subscriptions of
                Just subscription ->
                    let
                        updatedSubscription =
                            { subscription | input = newInput }

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



-- Notify that tables have changed


notifyTablesChanged : Data.Schema.SchemaMetadata -> Db.Db -> Model -> List String -> List (Cmd msg)
notifyTablesChanged schema db model affectedTables =
    Dict.foldl
        (\queryId subscription acc ->
            let
                queryTables =
                    extractQueryTables schema subscription.query

                isAffected =
                    List.any (\table -> List.member table queryTables) affectedTables
            in
            if isAffected then
                let
                    result =
                        Db.executeQuery schema db subscription.query

                    resultJson =
                        encodeQueryResult result
                in
                queryResult subscription.callbackPort resultJson :: acc

            else
                acc
        )
        []
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
                        Decode.map2 UpdateQueryInput
                            (Decode.field "queryId" Decode.string)
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

