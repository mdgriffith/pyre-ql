module Data.Schema exposing (LinkInfo, LinkTarget, LinkType(..), SchemaMetadata, TableMetadata, decodeLinkInfo, decodeLinkTarget, decodeLinkType, decodeSchemaMetadata, decodeTableMetadata)

import Dict exposing (Dict)
import Json.Decode as Decode


type alias LinkInfo =
    { type_ : LinkType
    , from : String
    , to : LinkTarget
    }


type LinkType
    = ManyToOne
    | OneToMany
    | OneToOne


type alias LinkTarget =
    { table : String
    , column : String
    }


type alias TableMetadata =
    { name : String
    , links : Dict String LinkInfo
    }


type alias SchemaMetadata =
    { tables : Dict String TableMetadata
    , queryFieldToTable : Dict String String
    }


decodeLinkType : Decode.Decoder LinkType
decodeLinkType =
    Decode.string
        |> Decode.andThen
            (\str ->
                case str of
                    "many-to-one" ->
                        Decode.succeed ManyToOne

                    "one-to-many" ->
                        Decode.succeed OneToMany

                    "one-to-one" ->
                        Decode.succeed OneToOne

                    _ ->
                        Decode.fail ("Unknown link type: " ++ str)
            )


decodeLinkInfo : Decode.Decoder LinkInfo
decodeLinkInfo =
    Decode.map3 LinkInfo
        (Decode.field "type" decodeLinkType)
        (Decode.field "from" Decode.string)
        (Decode.field "to" decodeLinkTarget)


decodeLinkTarget : Decode.Decoder LinkTarget
decodeLinkTarget =
    Decode.map2 LinkTarget
        (Decode.field "table" Decode.string)
        (Decode.field "column" Decode.string)


decodeTableMetadata : Decode.Decoder TableMetadata
decodeTableMetadata =
    Decode.map2 TableMetadata
        (Decode.field "name" Decode.string)
        (Decode.field "links" (Decode.dict decodeLinkInfo))


decodeSchemaMetadata : Decode.Decoder SchemaMetadata
decodeSchemaMetadata =
    Decode.map2 SchemaMetadata
        (Decode.field "tables" (Decode.dict decodeTableMetadata))
        (Decode.field "queryFieldToTable" (Decode.dict Decode.string))
