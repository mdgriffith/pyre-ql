module Db.Encode exposing(..)

import Db
import Json.Encode as Encode


encodeStatus : Db.Status -> Encode.Value
encodeStatus input_ =
    case input_ of
        Db.Active ->
            Encode.object [ ( "type", Encode.string "Active" ) ]

        Db.Inactive ->
            Encode.object [ ( "type", Encode.string "Inactive" ) ]

        Db.Special inner_details__ ->
            Encode.object
                [ ( "type", Encode.string "Special" )
                , ( "reason", Encode.string inner_details__.reason)
                ]

        Db.Inactive ->
            Encode.object [ ( "type", Encode.string "Inactive" ) ]

        Db.Special2 inner_details__ ->
            Encode.object
                [ ( "type", Encode.string "Special2" )
                , ( "reason2", Encode.string inner_details__.reason2)
                , ( "error", Encode.string inner_details__.error)
                ]

