port module Data.Error exposing (sendError)

import Json.Encode as Encode


port errorOut : String -> Cmd msg


sendError : String -> Cmd msg
sendError message =
    errorOut message
