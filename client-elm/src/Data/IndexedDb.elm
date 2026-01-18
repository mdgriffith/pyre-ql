port module Data.IndexedDb exposing
    ( Incoming(..)
    , InitialData
    , receiveIncoming
    , requestInitialData
    , writeDelta
    )

import Data.Delta exposing (TableGroup)
import Data.Value exposing (Value)
import Dict exposing (Dict)
import Json.Decode as Decode
import Json.Encode as Encode



-- IndexedDB-specific types


type alias InitialData =
    { tables : Dict String (List (Dict String Value))
    }



-- tableName -> (id -> row)
-- IndexedDB Message Types


type Message
    = RequestInitialData
    | WriteDelta (List TableGroup)


type Incoming
    = InitialDataReceived InitialData



-- Ports


port indexedDbOut : Encode.Value -> Cmd msg


port receiveIndexedDbMessage : (Decode.Value -> msg) -> Sub msg



-- Encoders


encodeMessage : Message -> Encode.Value
encodeMessage msg =
    case msg of
        RequestInitialData ->
            Encode.object
                [ ( "type", Encode.string "requestInitialData" )
                ]

        WriteDelta tableGroups ->
            Encode.object
                [ ( "type", Encode.string "writeDelta" )
                , ( "tableGroups", Encode.list Data.Delta.encodeTableGroup tableGroups )
                ]



-- Decoders


decodeIncoming : Decode.Decoder Incoming
decodeIncoming =
    Decode.field "type" Decode.string
        |> Decode.andThen
            (\type_ ->
                case type_ of
                    "initialData" ->
                        Decode.field "data" decodeInitialData
                            |> Decode.map InitialDataReceived

                    _ ->
                        Decode.fail ("Unknown IndexedDB incoming type: " ++ type_)
            )


decodeInitialData : Decode.Decoder InitialData
decodeInitialData =
    Decode.map InitialData
        (Decode.field "tables"
            (Decode.dict (Decode.list (Decode.dict Data.Value.decodeValue)))
        )



-- Helper functions


sendMessage : Message -> Cmd msg
sendMessage msg =
    indexedDbOut (encodeMessage msg)


requestInitialData : Cmd msg
requestInitialData =
    sendMessage RequestInitialData


writeDelta : List TableGroup -> Cmd msg
writeDelta tableGroups =
    sendMessage (WriteDelta tableGroups)


receiveIncoming : (Result Decode.Error Incoming -> msg) -> Sub msg
receiveIncoming toMsg =
    receiveIndexedDbMessage (\jsonValue -> toMsg (Decode.decodeValue decodeIncoming jsonValue))

