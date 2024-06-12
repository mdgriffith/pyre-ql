module Db.Read exposing
    ( Query, query
    , Decoder(..), succeed, field
    , bool, string, int, float
    , id, nested
    , decodeValue
    )

{-|

@docs Query, query

@docs Decoder, succeed, field

@docs bool, string, int, float

@docs id, nested

@docs decodeValue

-}

import Json.Decode as Json
import Set exposing (Set)


type Decoder a
    = Decoder (Json.Value -> Json.Decoder a)


type Query a
    = Query
        { identity : List (Json.Decoder Id)
        , decoder : Decoder a
        }


decodeValue : Query selected -> Json.Value -> Result Json.Error (List selected)
decodeValue (Query queryDetails) json =
    runDecoderWith queryDetails.identity queryDetails.decoder (Json.succeed True) json


query : a -> List (IdField Id) -> Query a
query decoder identity =
    Query
        { identity = List.map .decoder identity
        , decoder = succeed decoder
        }


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


field : String -> Decoder a -> Query (a -> b) -> Query b
field fieldName_ (Decoder toFieldDecoder) (Query q) =
    let
        (Decoder toBuilder) =
            q.decoder
    in
    Query
        { identity = q.identity
        , decoder =
            Decoder
                (\json ->
                    toBuilder json
                        |> Json.andThen
                            (\func ->
                                Json.field fieldName_ (toFieldDecoder json)
                                    |> Json.map func
                            )
                )
        }


type alias Id =
    Int


type alias IdField val =
    { name : String
    , decoder : Json.Decoder val
    }


id : String -> IdField Id
id name =
    { name = name
    , decoder = Json.field name Json.int
    }


toJsonDecoder : Json.Value -> Decoder a -> Json.Decoder a
toJsonDecoder json (Decoder toInner) =
    toInner json


nested : String -> IdField Id -> IdField Id -> Query innerSelected -> Query (List innerSelected -> selected) -> Query selected
nested fieldName topLevelIdField innerId (Query innerQ) (Query topQ) =
    Query
        { identity = topQ.identity
        , decoder =
            Decoder
                (\fullJson ->
                    Json.oneOf
                        [ topLevelIdField.decoder
                            |> Json.andThen
                                (\parentId ->
                                    let
                                        rowCheck =
                                            Json.map
                                                (\foundParentId ->
                                                    foundParentId == parentId
                                                )
                                                topLevelIdField.decoder
                                    in
                                    case runDecoderWith innerQ.identity innerQ.decoder rowCheck fullJson of
                                        Ok captured ->
                                            Json.map
                                                (\fn ->
                                                    fn captured
                                                )
                                                (toJsonDecoder fullJson topQ.decoder)

                                        Err err ->
                                            Json.fail "Failed decoding nested columns"
                                )
                        , Json.map
                            (\fn ->
                                fn []
                            )
                            (toJsonDecoder fullJson topQ.decoder)
                        ]
                )
        }


runDecoderWith : List (Json.Decoder Id) -> Decoder selected -> Json.Decoder Bool -> Json.Value -> Result Json.Error (List selected)
runDecoderWith uniqueBy (Decoder toDecoder) rowCheckDecoder json =
    Json.decodeValue
        (uniqueListDecoder uniqueBy rowCheckDecoder (toDecoder json))
        json
        |> Result.map List.reverse


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
idToString =
    String.fromInt
