module Data.Value exposing (Value(..), decodeValue, encodeValue)

import Dict exposing (Dict)
import Json.Decode as Decode
import Json.Encode as Encode


type Value
    = StringValue String
    | IntValue Int
    | FloatValue Float
    | BoolValue Bool
    | NullValue
    | ArrayValue (List Value)
    | ObjectValue (Dict String Value)


decodeValue : Decode.Decoder Value
decodeValue =
    Decode.oneOf
        [ Decode.string |> Decode.map StringValue
        , Decode.int |> Decode.map IntValue
        , Decode.float |> Decode.map FloatValue
        , Decode.bool |> Decode.map BoolValue
        , Decode.null NullValue
        , Decode.list (Decode.lazy (\_ -> decodeValue)) |> Decode.map ArrayValue
        , Decode.dict (Decode.lazy (\_ -> decodeValue)) |> Decode.map ObjectValue
        ]


encodeValue : Value -> Encode.Value
encodeValue value =
    case value of
        StringValue str ->
            Encode.string str

        IntValue i ->
            Encode.int i

        FloatValue f ->
            Encode.float f

        BoolValue b ->
            Encode.bool b

        NullValue ->
            Encode.null

        ArrayValue items ->
            Encode.list encodeValue items

        ObjectValue dict ->
            Encode.dict identity encodeValue dict

