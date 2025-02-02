module Json.Decode.Help exposing (andField)

import Json.Decode as Decode



{-| Chain field decoders together, similar to Db.Read.field.
This allows you to build up a decoder by adding fields one at a time.

    decodeGame =
        Decode.succeed Game
            |> andField "id" Decode.int 
            |> andField "name" Decode.string

-}
andField : String -> Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andField field decoder partial =
    Decode.map2 (\f value -> f value)
        partial
        (Decode.field field decoder)
