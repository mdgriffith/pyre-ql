module Db.Read exposing
    ( Decoder, succeed, field
    , bool, string, int, float
    , decodeValue
    )

{-|

@docs Decoder, succeed, field

@docs bool, string, int, float

-}

import Json.Decode as Json
import Set exposing (Set)


type Decoder a
    = Decoder (Json.Value -> Json.Decoder a)


decodeValue : Decoder selected -> Json.Value -> Result Json.Error (List selected)
decodeValue (Decoder toInner) json =
    Json.decodeValue (Json.list (toInner json)) json


succeed : a -> Decoder a
succeed a =
    Decoder (\_ -> Json.succeed a)


nullable : Decoder a -> Decoder (Maybe a)
nullable (Decoder toInner) =
    Decoder (\json -> Json.nullable (toInner json))


int : Decoder Int
int =
    Decoder (\_ -> Json.int)


string : Decoder String
string =
    Decoder (\_ -> Json.string)


bool : Decoder Bool
bool =
    Decoder
        (\_ ->
            Json.map (\i -> i /= 0) Json.int
        )


float : Decoder Float
float =
    Decoder (\_ -> Json.float)


field : String -> Decoder a -> Decoder (a -> b) -> Decoder b
field fieldName_ (Decoder toFieldDecoder) (Decoder toBuilder) =
    Decoder
        (\json ->
            toBuilder json
                |> Json.andThen
                    (\func ->
                        Json.field fieldName_ (toFieldDecoder json)
                            |> Json.map func
                    )
        )


type alias Id =
    Int


uniqueListDecoder : List (Json.Decoder Id) -> Json.Decoder Bool -> Json.Decoder selected -> Json.Decoder (List selected)
uniqueListDecoder uniqueBy rowCheck decoder =
    uniqueListDecoderHelper 0 Set.empty uniqueBy rowCheck [] decoder


uniqueListDecoderHelper : Int -> Set String -> List (Json.Decoder Id) -> Json.Decoder Bool -> List selected -> Json.Decoder selected -> Json.Decoder (List selected)
uniqueListDecoderHelper index found uniqueBy rowCheck foundList rowDecoder =
    Json.oneOf
        [ Json.index index rowCheck
            |> Json.andThen
                (\should_include ->
                    if should_include then
                        Json.index index (decodeCompoundId uniqueBy)
                            |> Json.andThen
                                (\compoundId ->
                                    Json.lazy
                                        (\_ ->
                                            if Set.member compoundId found && compoundId /= "" then
                                                uniqueListDecoderHelper (index + 1) found uniqueBy rowCheck foundList rowDecoder

                                            else
                                                Json.index index rowDecoder
                                                    |> Json.andThen
                                                        (\row ->
                                                            uniqueListDecoderHelper (index + 1)
                                                                (Set.insert compoundId found)
                                                                uniqueBy
                                                                rowCheck
                                                                (row :: foundList)
                                                                rowDecoder
                                                        )
                                        )
                                )

                    else
                        Json.fail "Failed row check"
                )
        , -- If indices still exist, keep going
          Json.index index (Json.succeed ())
            |> Json.andThen
                (\_ ->
                    Json.lazy
                        (\_ ->
                            uniqueListDecoderHelper (index + 1) found uniqueBy rowCheck foundList rowDecoder
                        )
                )
        , Json.lazy
            (\_ ->
                Json.succeed foundList
            )
        ]


decodeCompoundId : List (Json.Decoder Id) -> Json.Decoder String
decodeCompoundId uniqueBy =
    decodeCompoundIdHelper uniqueBy ""


decodeCompoundIdHelper : List (Json.Decoder Id) -> String -> Json.Decoder String
decodeCompoundIdHelper idDecoder str =
    case idDecoder of
        [] ->
            Json.succeed str

        decoder :: rest ->
            decoder
                |> Json.andThen
                    (\foundId ->
                        Json.lazy
                            (\_ ->
                                decodeCompoundIdHelper rest (str ++ "_" ++ idToString foundId)
                            )
                    )


idToString : Id -> String
idToString id =
    String.fromInt id
