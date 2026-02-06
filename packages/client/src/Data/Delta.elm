module Data.Delta exposing (Delta, TableGroup, decodeDelta, decodeTableGroup, encodeDelta, encodeTableGroup)

import Data.Value exposing (Value)
import Json.Decode as Decode
import Json.Encode as Encode


{-| Delta contains table groups, where each group has multiple rows for a single table.
This grouped format is more efficient than sending individual rows.
-}
type alias Delta =
    { tableGroups : List TableGroup
    }


{-| A group of rows for a single table.
Rows are stored as arrays (not objects) to minimize bandwidth.
Headers provide the mapping from array indices to column names.
-}
type alias TableGroup =
    { tableName : String
    , headers : List String
    , rows : List (List Value)
    }


decodeDelta : Decode.Decoder Delta
decodeDelta =
    Decode.map Delta
        (Decode.list decodeTableGroup)


decodeTableGroup : Decode.Decoder TableGroup
decodeTableGroup =
    Decode.map3 TableGroup
        (Decode.field "table_name" Decode.string)
        (Decode.field "headers" (Decode.list Decode.string))
        (Decode.field "rows" (Decode.list (Decode.list Data.Value.decodeValue)))


encodeDelta : Delta -> Encode.Value
encodeDelta delta =
    Encode.list encodeTableGroup delta.tableGroups


encodeTableGroup : TableGroup -> Encode.Value
encodeTableGroup group =
    Encode.object
        [ ( "table_name", Encode.string group.tableName )
        , ( "headers", Encode.list Encode.string group.headers )
        , ( "rows", Encode.list (Encode.list Data.Value.encodeValue) group.rows )
        ]

