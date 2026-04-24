port module Data.IndexedDb exposing
    ( Incoming(..)
    , InitialData
    , SyncCursor
    , receiveIncoming
    , requestInitialData
    , writeDelta
    , writeSyncCursor
    )

import Data.Delta exposing (TableGroup)
import Data.Value exposing (Value)
import Dict exposing (Dict)
import Json.Decode as Decode
import Json.Encode as Encode



-- IndexedDB-specific types


type alias InitialData =
    { tables : Dict String (List (Dict String Value))
    , cursor : SyncCursor
    }


type alias SyncCursor =
    Dict String SyncCursorEntry


type alias SyncCursorEntry =
    { lastSeenUpdatedAt : Maybe Float
    , permissionHash : String
    }



-- tableName -> (id -> row)
-- IndexedDB Message Types


type Message
    = RequestInitialData
    | WriteDelta (List TableGroup)
    | WriteSyncCursor SyncCursor


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

        WriteSyncCursor cursor ->
            Encode.object
                [ ( "type", Encode.string "writeSyncCursor" )
                , ( "cursor", encodeSyncCursor cursor )
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
    Decode.map2 InitialData
        (Decode.field "tables" (Decode.dict (Decode.list (Decode.dict Data.Value.decodeValue))))
        (Decode.field "cursor" decodeSyncCursor)


decodeSyncCursor : Decode.Decoder SyncCursor
decodeSyncCursor =
    Decode.field "tables" (Decode.dict decodeSyncCursorEntry)


decodeSyncCursorEntry : Decode.Decoder SyncCursorEntry
decodeSyncCursorEntry =
    Decode.map2 SyncCursorEntry
        (Decode.field "last_seen_updated_at" decodeMaybeTimestamp)
        (Decode.field "permission_hash" Decode.string)


decodeMaybeTimestamp : Decode.Decoder (Maybe Float)
decodeMaybeTimestamp =
    Decode.oneOf
        [ Decode.null Nothing
        , Decode.float |> Decode.map Just
        , Decode.int |> Decode.map (toFloat >> Just)
        ]


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


writeSyncCursor : SyncCursor -> Cmd msg
writeSyncCursor cursor =
    sendMessage (WriteSyncCursor cursor)


receiveIncoming : (Result Decode.Error Incoming -> msg) -> Sub msg
receiveIncoming toMsg =
    receiveIndexedDbMessage (\jsonValue -> toMsg (Decode.decodeValue decodeIncoming jsonValue))
