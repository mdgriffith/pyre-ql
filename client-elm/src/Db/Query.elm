module Db.Query exposing
    ( FieldQuery
    , FilterValue(..)
    , Query
    , Selection(..)
    , SortClause
    , SortDirection(..)
    , WhereClause
    , decodeQuery
    )

import Data.Value exposing (Value)
import Dict exposing (Dict)
import Json.Decode as Decode


type alias Query =
    Dict String FieldQuery


type alias FieldQuery =
    { selections : Dict String Selection
    , where_ : Maybe WhereClause
    , sort : Maybe (List SortClause)
    , limit : Maybe Int
    }


type Selection
    = SelectField
    | SelectNested FieldQuery


type alias WhereClause =
    Dict String FilterValue


type FilterValue
    = FilterValueSimple Value
    | FilterValueNull
    | FilterValueOperators (Dict String FilterValue)
    | FilterValueAnd (List WhereClause)
    | FilterValueOr (List WhereClause)


type alias SortClause =
    { field : String
    , direction : SortDirection
    }


type SortDirection
    = Asc
    | Desc


decodeQuery : Decode.Decoder Query
decodeQuery =
    Decode.dict decodeFieldQuery


decodeFieldQuery : Decode.Decoder FieldQuery
decodeFieldQuery =
    Decode.keyValuePairs Decode.value
        |> Decode.andThen buildFieldQueryFromPairs


buildFieldQueryFromPairs : List ( String, Decode.Value ) -> Decode.Decoder FieldQuery
buildFieldQueryFromPairs pairs =
    buildFieldQueryFromPairsHelp Dict.empty Nothing Nothing Nothing pairs


buildFieldQueryFromPairsHelp :
    Dict String Selection
    -> Maybe WhereClause
    -> Maybe (List SortClause)
    -> Maybe Int
    -> List ( String, Decode.Value )
    -> Decode.Decoder FieldQuery
buildFieldQueryFromPairsHelp selections where_ sort limit pairs =
    case pairs of
        [] ->
            Decode.succeed
                { selections = selections
                , where_ = where_
                , sort = sort
                , limit = limit
                }

        ( key, value ) :: rest ->
            case key of
                "@where" ->
                    case Decode.decodeValue decodeWhereClause value of
                        Ok whereClause ->
                            buildFieldQueryFromPairsHelp selections (Just whereClause) sort limit rest

                        Err _ ->
                            Decode.fail "Invalid @where clause"

                "@sort" ->
                    case Decode.decodeValue decodeSortValue value of
                        Ok sortClauses ->
                            buildFieldQueryFromPairsHelp selections where_ (Just sortClauses) limit rest

                        Err _ ->
                            Decode.fail "Invalid @sort clause"

                "@limit" ->
                    case Decode.decodeValue Decode.int value of
                        Ok limitValue ->
                            buildFieldQueryFromPairsHelp selections where_ sort (Just limitValue) rest

                        Err _ ->
                            Decode.fail "Invalid @limit value"

                _ ->
                    -- Regular field selection (bool or nested)
                    case Decode.decodeValue decodeSelection value of
                        Ok selection ->
                            buildFieldQueryFromPairsHelp (Dict.insert key selection selections) where_ sort limit rest

                        Err _ ->
                            -- Skip invalid selections
                            buildFieldQueryFromPairsHelp selections where_ sort limit rest


decodeSelection : Decode.Decoder Selection
decodeSelection =
    Decode.oneOf
        [ Decode.bool
            |> Decode.andThen
                (\b ->
                    if b then
                        Decode.succeed SelectField

                    else
                        Decode.fail "false is not a valid selection"
                )
        , Decode.lazy (\_ -> decodeFieldQuery)
            |> Decode.map SelectNested
        ]


decodeSortValue : Decode.Decoder (List SortClause)
decodeSortValue =
    Decode.oneOf
        [ Decode.list decodeSortClause
        , decodeSortClause |> Decode.map (\s -> [ s ])
        ]


decodeWhereClause : Decode.Decoder WhereClause
decodeWhereClause =
    Decode.dict decodeFilterValue


decodeFilterValue : Decode.Decoder FilterValue
decodeFilterValue =
    Decode.oneOf
        [ Decode.null FilterValueNull
        , Decode.dict (Decode.lazy (\_ -> decodeFilterValue))
            |> Decode.andThen
                (\dict ->
                    if Dict.member "$and" dict then
                        Decode.field "$and" (Decode.list decodeWhereClause)
                            |> Decode.map FilterValueAnd

                    else if Dict.member "$or" dict then
                        Decode.field "$or" (Decode.list decodeWhereClause)
                            |> Decode.map FilterValueOr

                    else
                        Decode.succeed (FilterValueOperators dict)
                )
        , Data.Value.decodeValue |> Decode.map FilterValueSimple
        ]


decodeSortClause : Decode.Decoder SortClause
decodeSortClause =
    Decode.map2 SortClause
        (Decode.field "field" Decode.string)
        (Decode.field "direction" decodeSortDirection)


decodeSortDirection : Decode.Decoder SortDirection
decodeSortDirection =
    Decode.string
        |> Decode.andThen
            (\str ->
                case String.toLower str of
                    "desc" ->
                        Decode.succeed Desc

                    _ ->
                        Decode.succeed Asc
            )

