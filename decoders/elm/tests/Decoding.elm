module Decoding exposing (..)

import Json.Encode as Encode
import Expect exposing (Expectation)
import Fuzz exposing (Fuzzer, int, list, string)
import Test exposing (Test)
import Db.Read


type alias User =
   { id : Int
   , email : String
   , rating : Float
   }


encodeUser : User -> Encode.Value
encodeUser user =
    Encode.object
        [ ( "id", Encode.int user.id )
        , ( "email", Encode.string user.email )
        , ( "rating", Encode.float user.rating )
        ]

suite : Test.Test
suite =
    Test.describe "Rectangular Results"
      [ Test.test "Basic Decoding (no nesting)" <|
        \_ ->
          let

            users =
              [ User 1 "email" 5.0
              , User 2 "email2" 7.0
              , User 3 "email3" 10.0
              ]

            data =
                Encode.list encodeUser users


            userDecoder =
                Db.Read.succeed User
                    |> Db.Read.field "id" Db.Read.int
                    |> Db.Read.field "email" Db.Read.string
                    |> Db.Read.field "rating" Db.Read.float
          in
          Expect.equal
            (Db.Read.decodeValue userDecoder data)
            (Ok users)


      ]
