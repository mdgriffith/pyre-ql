module Db exposing(..)

type alias User =
    { id : Int
    , name : String
    , status : Status
    }


type Status
   = Active
   | Inactive
   | Special
      { reason : String
      }
   | Inactive
   | Special2
      { reason2 : String
      , error : String
      }


type alias Account =
    { id : Int
    , userId : Int
    , name : String
    , status : Status
    }
