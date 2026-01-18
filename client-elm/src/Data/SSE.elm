port module Data.SSE exposing
    ( Incoming(..)
    , Message
    , SSEConfig
    , SyncProgress
    , connect
    , decodeSSEConfig
    , receiveIncoming
    )

import Data.Delta exposing (Delta)
import Json.Decode as Decode
import Json.Encode as Encode


type alias SSEConfig =
    { baseUrl : String
    , userId : Int
    }


type alias SyncProgress =
    { table : Maybe String
    , tablesSynced : Int
    , totalTables : Maybe Int
    , complete : Bool
    , error : Maybe String
    }



-- SSE Message Types


type Message
    = ConnectSSE SSEConfig
    | DisconnectSSE


type Incoming
    = DeltaReceived Delta
    | SyncProgressReceived SyncProgress
    | SSEConnected String
    | SSEError String
    | SyncCompleteReceived



-- Ports


port sseOut : Encode.Value -> Cmd msg


port receiveSSEMessage : (Decode.Value -> msg) -> Sub msg



-- Encoders


encodeMessage : Message -> Encode.Value
encodeMessage msg =
    case msg of
        ConnectSSE config ->
            Encode.object
                [ ( "type", Encode.string "connectSSE" )
                , ( "config", encodeSSEConfig config )
                ]

        DisconnectSSE ->
            Encode.object
                [ ( "type", Encode.string "disconnectSSE" )
                ]


encodeSSEConfig : SSEConfig -> Encode.Value
encodeSSEConfig config =
    Encode.object
        [ ( "baseUrl", Encode.string config.baseUrl )
        , ( "userId", Encode.int config.userId )
        ]



-- Decoders


decodeIncoming : Decode.Decoder Incoming
decodeIncoming =
    Decode.field "type" Decode.string
        |> Decode.andThen
            (\type_ ->
                case type_ of
                    "delta" ->
                        Decode.field "data" Data.Delta.decodeDelta
                            |> Decode.map DeltaReceived

                    "syncProgress" ->
                        Decode.field "data" decodeSyncProgress
                            |> Decode.map SyncProgressReceived

                    "connected" ->
                        Decode.field "sessionId" Decode.string
                            |> Decode.map SSEConnected

                    "error" ->
                        Decode.field "error" Decode.string
                            |> Decode.map SSEError

                    "syncComplete" ->
                        Decode.succeed SyncCompleteReceived

                    _ ->
                        Decode.fail ("Unknown SSE incoming type: " ++ type_)
            )


decodeSSEConfig : Decode.Decoder SSEConfig
decodeSSEConfig =
    Decode.map2 SSEConfig
        (Decode.field "baseUrl" Decode.string)
        (Decode.field "userId" Decode.int)


decodeSyncProgress : Decode.Decoder SyncProgress
decodeSyncProgress =
    Decode.map5 SyncProgress
        (Decode.maybe (Decode.field "table" Decode.string))
        (Decode.field "tablesSynced" Decode.int)
        (Decode.maybe (Decode.field "totalTables" Decode.int))
        (Decode.field "complete" Decode.bool)
        (Decode.maybe (Decode.field "error" Decode.string))



-- Helper functions


sendMessage : Message -> Cmd msg
sendMessage msg =
    sseOut (encodeMessage msg)


receiveIncoming : (Result Decode.Error Incoming -> msg) -> Sub msg
receiveIncoming toMsg =
    receiveSSEMessage (\jsonValue -> toMsg (Decode.decodeValue decodeIncoming jsonValue))


connect : SSEConfig -> Cmd msg
connect config =
    sendMessage (ConnectSSE config)
