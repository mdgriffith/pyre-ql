module Db.Updates exposing (Update(..), null, object, set, skip)

import Json.Encode as Encode


type Update a
    = Set a
    | Unchanged
    | SetToNull


set : a -> Update a
set value =
    Set value


skip : Update a
skip =
    Unchanged


null : Update a
null =
    SetToNull


object : List ( String, Update Encode.Value ) -> Encode.Value
object fields =
    fields
        |> List.filterMap
            (\( key, update ) ->
                case update of
                    Set value ->
                        Just ( key, value )

                    Unchanged ->
                        Nothing

                    SetToNull ->
                        Just ( key, Encode.null )
            )
        |> Encode.object
