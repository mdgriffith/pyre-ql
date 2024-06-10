module Db.Decode exposing(..)

import Db
import Json.Decode as Decode



type alias Status_Special =
    { reason : String
    }


type alias Status_Special2 =
    { reason2 : String
    , error : String
    }


decodeStatus : Decode.Decoder Db.Status
decodeStatus =
    Decode.field "type" Decode.string
        |> Decode.andThen
            (\variant_name ->
               case variant_name of
                  "Active" ->
                      Decode.succeed Db.Active

                  "Inactive" ->
                      Decode.succeed Db.Inactive

                  "Special" ->
                      Decode.map Db.Special
                          (Decode.succeed Status_Special
                              |> Decode.field "reason" Decode.string
                          )

                  "Inactive" ->
                      Decode.succeed Db.Inactive

                  "Special2" ->
                      Decode.map Db.Special2
                          (Decode.succeed Status_Special2
                              |> Decode.field "reason2" Decode.string
                              |> Decode.field "error" Decode.string
                          )

            )


