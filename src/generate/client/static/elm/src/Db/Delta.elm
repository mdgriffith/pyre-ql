module Db.Delta exposing (DeltaOp(..), decodeDeltaOp, PathSegment(..), parsePath, parsePathSegments, getAtIndex, setAtIndex, removeAtIndex, insertAtIndex, moveInList, ListLens, MaybeLens, FieldHandler, listField, maybeField, noNested, applyOps)


import Json.Decode as Decode


{-| Delta operations that can be applied to query results.
Paths are pre-parsed: (topLevelField, remainingSegments)
-}
type DeltaOp
    = SetRow ( String, List PathSegment ) Decode.Value
    | RemoveRow ( String, List PathSegment )
    | InsertRow ( String, List PathSegment ) Int Decode.Value
    | MoveRow ( String, List PathSegment ) Int Int
    | RemoveRowByIndex ( String, List PathSegment ) Int


decodePath : Decode.Decoder ( String, List PathSegment )
decodePath =
    Decode.string
        |> Decode.andThen
            (\pathStr ->
                case parsePath pathStr of
                    Just ( field, rest ) ->
                        Decode.succeed ( field, parsePathSegments rest )

                    Nothing ->
                        Decode.fail ("Invalid path: " ++ pathStr)
            )


decodeDeltaOp : Decode.Decoder DeltaOp
decodeDeltaOp =
    Decode.field "op" Decode.string
        |> Decode.andThen
            (\op_ ->
                case op_ of
                    "set-row" ->
                        Decode.map2 SetRow
                            (Decode.field "path" decodePath)
                            (Decode.field "row" Decode.value)

                    "remove-row" ->
                        Decode.map RemoveRow
                            (Decode.field "path" decodePath)

                    "insert-row" ->
                        Decode.map3 InsertRow
                            (Decode.field "path" decodePath)
                            (Decode.field "index" Decode.int)
                            (Decode.field "row" Decode.value)

                    "move-row" ->
                        Decode.map3 MoveRow
                            (Decode.field "path" decodePath)
                            (Decode.field "from" Decode.int)
                            (Decode.field "to" Decode.int)

                    "remove-row-by-index" ->
                        Decode.map2 RemoveRowByIndex
                            (Decode.field "path" decodePath)
                            (Decode.field "index" Decode.int)

                    _ ->
                        Decode.fail ("Unknown DeltaOp: " ++ op_)
            )


{-| A segment in a delta path.
-}
type PathSegment
    = Field String
    | Index Int
    | Id String


{-| Parse the first field segment from a path, returning the field name and remaining path.
    parsePath ".user[0].posts" == Just ( "user", "[0].posts" )
    parsePath ".post#(10).author" == Just ( "post", "#(10).author" )
-}
parsePath : String -> Maybe ( String, String )
parsePath path =
    case String.uncons path of
        Just ( '.', rest ) ->
            parseFieldName rest

        _ ->
            Nothing


parseFieldName : String -> Maybe ( String, String )
parseFieldName str =
    let
        isFieldChar c =
            Char.isAlphaNum c || c == '_'

        ( fieldName, rest ) =
            splitWhile isFieldChar str
    in
    if String.isEmpty fieldName then
        Nothing
    else
        Just ( fieldName, rest )


splitWhile : (Char -> Bool) -> String -> ( String, String )
splitWhile pred str =
    splitWhileHelp pred str ""


splitWhileHelp : (Char -> Bool) -> String -> String -> ( String, String )
splitWhileHelp pred remaining acc =
    case String.uncons remaining of
        Just ( c, rest ) ->
            if pred c then
                splitWhileHelp pred rest (acc ++ String.fromChar c)
            else
                ( acc, remaining )

        Nothing ->
            ( acc, remaining )


{-| Parse all path segments from a string (after the initial field).
    parsePathSegments "[0].posts[1]" == [ Index 0, Field "posts", Index 1 ]
    parsePathSegments "#(10).author" == [ Id "10", Field "author" ]
-}
parsePathSegments : String -> List PathSegment
parsePathSegments path =
    parsePathSegmentsHelp path []


parsePathSegmentsHelp : String -> List PathSegment -> List PathSegment
parsePathSegmentsHelp path acc =
    if String.isEmpty path then
        List.reverse acc
    else
        case String.uncons path of
            Just ( '[', rest ) ->
                -- Parse index: [0], [1], etc.
                case parseIndexSegment rest of
                    Just ( idx, remaining ) ->
                        parsePathSegmentsHelp remaining (Index idx :: acc)

                    Nothing ->
                        List.reverse acc

            Just ( '#', rest ) ->
                -- Parse id selector: #(10), #(user-123)
                case parseIdSegment rest of
                    Just ( id, remaining ) ->
                        parsePathSegmentsHelp remaining (Id id :: acc)

                    Nothing ->
                        List.reverse acc

            Just ( '.', rest ) ->
                -- Parse field name
                case parseFieldName rest of
                    Just ( fieldName, remaining ) ->
                        parsePathSegmentsHelp remaining (Field fieldName :: acc)

                    Nothing ->
                        List.reverse acc

            _ ->
                List.reverse acc


parseIndexSegment : String -> Maybe ( Int, String )
parseIndexSegment str =
    let
        ( numStr, rest ) =
            splitWhile Char.isDigit str
    in
    case ( String.toInt numStr, String.uncons rest ) of
        ( Just idx, Just ( ']', remaining ) ) ->
            Just ( idx, remaining )

        _ ->
            Nothing


parseIdSegment : String -> Maybe ( String, String )
parseIdSegment str =
    case String.uncons str of
        Just ( '(', rest ) ->
            parseIdContent rest ""

        _ ->
            Nothing


parseIdContent : String -> String -> Maybe ( String, String )
parseIdContent str acc =
    case String.uncons str of
        Just ( ')', rest ) ->
            Just ( acc, rest )

        Just ( '\\', rest ) ->
            -- Handle escaping
            case String.uncons rest of
                Just ( c, remaining ) ->
                    parseIdContent remaining (acc ++ String.fromChar c)

                Nothing ->
                    Nothing

        Just ( c, rest ) ->
            parseIdContent rest (acc ++ String.fromChar c)

        Nothing ->
            Nothing


{-| Apply a function at a specific path within a list, handling nested structures.
    The function receives the remaining path segments and the item at the current index.
-}
applyAtPath : List PathSegment -> (List PathSegment -> a -> Result String a) -> List a -> Result String (List a)
applyAtPath segments fn list =
    case segments of
        [] ->
            -- No more segments, this shouldn't happen in normal use
            Ok list

        (Index idx) :: rest ->
            case getAtIndex idx list of
                Just item ->
                    case fn rest item of
                        Ok newItem ->
                            Ok (setAtIndex idx newItem list)

                        Err err ->
                            Err err

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id _) :: _ ->
            -- Id lookups need to be handled by the specific type's applier
            Err "Id selectors must be handled by type-specific code"

        (Field _) :: _ ->
            -- Field access at list level doesn't make sense
            Err "Cannot access field on a list"


{-| Get an item at a specific index in a list.
-}
getAtIndex : Int -> List a -> Maybe a
getAtIndex idx list =
    List.drop idx list |> List.head


{-| Set an item at a specific index in a list.
-}
setAtIndex : Int -> a -> List a -> List a
setAtIndex idx item list =
    List.indexedMap
        (\i existing ->
            if i == idx then
                item
            else
                existing
        )
        list


{-| Remove an item at a specific index from a list.
-}
removeAtIndex : Int -> List a -> List a
removeAtIndex idx list =
    List.take idx list ++ List.drop (idx + 1) list


{-| Insert an item at a specific index in a list.
-}
insertAtIndex : Int -> a -> List a -> List a
insertAtIndex idx item list =
    List.take idx list ++ [ item ] ++ List.drop idx list


{-| Move an item from one index to another in a list.
-}
moveInList : Int -> Int -> List a -> List a
moveInList from to list =
    case getAtIndex from list of
        Just item ->
            let
                without =
                    removeAtIndex from list
            in
            insertAtIndex to item without

        Nothing ->
            list


-- LENS TYPES


{-| A lens for accessing a List field on a record.
-}
type alias ListLens record item =
    { get : record -> List item
    , set : List item -> record -> record
    , decode : Decode.Decoder item
    , nested : String -> Maybe (FieldHandler item)
    }


{-| A lens for accessing a Maybe field on a record.
-}
type alias MaybeLens record item =
    { get : record -> Maybe item
    , set : Maybe item -> record -> record
    , decode : Decode.Decoder item
    , nested : String -> Maybe (FieldHandler item)
    }


{-| A field handler encapsulates all delta operations for a single field.
This allows heterogeneous field types to be handled uniformly.
-}
type FieldHandler record
    = ListHandler (ListFieldHandler record)
    | MaybeHandler (MaybeFieldHandler record)


type alias ListFieldHandler record =
    { setRow : List PathSegment -> Decode.Value -> record -> Result String record
    , removeRow : List PathSegment -> record -> Result String record
    , insertRow : List PathSegment -> Int -> Decode.Value -> record -> Result String record
    , moveRow : List PathSegment -> Int -> Int -> record -> Result String record
    , removeByIndex : List PathSegment -> Int -> record -> Result String record
    }


type alias MaybeFieldHandler record =
    { setRow : List PathSegment -> Decode.Value -> record -> Result String record
    , removeRow : List PathSegment -> record -> Result String record
    }


-- LENS CONSTRUCTORS


{-| Create a FieldHandler for a List field.
-}
listField : ListLens record item -> FieldHandler record
listField lens =
    ListHandler
        { setRow = setRowInList lens
        , removeRow = removeRowFromList lens
        , insertRow = insertRowInList lens
        , moveRow = moveRowInListField lens
        , removeByIndex = removeByIndexFromList lens
        }


{-| Create a FieldHandler for a Maybe field.
-}
maybeField : MaybeLens record item -> FieldHandler record
maybeField lens =
    MaybeHandler
        { setRow = setRowInMaybe lens
        , removeRow = removeRowFromMaybe lens
        }


{-| Helper for fields with no nested children.
-}
noNested : String -> Maybe (FieldHandler item)
noNested _ =
    Nothing


-- LIST FIELD OPERATIONS


setRowInList : ListLens record item -> List PathSegment -> Decode.Value -> record -> Result String record
setRowInList lens segments json record =
    case segments of
        [] ->
            Err "Cannot set row without index"

        (Index idx) :: rest ->
            let
                list =
                    lens.get record
            in
            case getAtIndex idx list of
                Just item ->
                    if List.isEmpty rest then
                        case Decode.decodeValue lens.decode json of
                            Ok row ->
                                Ok (lens.set (setAtIndex idx row list) record)

                            Err err ->
                                Err ("Failed to decode row: " ++ Decode.errorToString err)

                    else
                        applyNestedSetRow lens.nested rest json item
                            |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id id) :: rest ->
            let
                list =
                    lens.get record
            in
            case findByIdWithIndex id list of
                Just ( idx, item ) ->
                    if List.isEmpty rest then
                        case Decode.decodeValue lens.decode json of
                            Ok row ->
                                Ok (lens.set (setAtIndex idx row list) record)

                            Err err ->
                                Err ("Failed to decode row: " ++ Decode.errorToString err)

                    else
                        applyNestedSetRow lens.nested rest json item
                            |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Field _) :: _ ->
            Err "Unexpected field segment at list level"


removeRowFromList : ListLens record item -> List PathSegment -> record -> Result String record
removeRowFromList lens segments record =
    case segments of
        [] ->
            Err "Cannot remove row without index"

        [ Index idx ] ->
            Ok (lens.set (removeAtIndex idx (lens.get record)) record)

        [ Id id ] ->
            case findByIdWithIndex id (lens.get record) of
                Just ( idx, _ ) ->
                    Ok (lens.set (removeAtIndex idx (lens.get record)) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Index idx) :: rest ->
            let
                list =
                    lens.get record
            in
            case getAtIndex idx list of
                Just item ->
                    applyNestedRemoveRow lens.nested rest item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id id) :: rest ->
            let
                list =
                    lens.get record
            in
            case findByIdWithIndex id list of
                Just ( idx, item ) ->
                    applyNestedRemoveRow lens.nested rest item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Field _) :: _ ->
            Err "Unexpected field segment at list level"


insertRowInList : ListLens record item -> List PathSegment -> Int -> Decode.Value -> record -> Result String record
insertRowInList lens segments index json record =
    case segments of
        [] ->
            -- Insert at this list
            case Decode.decodeValue lens.decode json of
                Ok row ->
                    Ok (lens.set (insertAtIndex index row (lens.get record)) record)

                Err err ->
                    Err ("Failed to decode row: " ++ Decode.errorToString err)

        (Index idx) :: rest ->
            let
                list =
                    lens.get record
            in
            case getAtIndex idx list of
                Just item ->
                    applyNestedInsertRow lens.nested rest index json item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id id) :: rest ->
            let
                list =
                    lens.get record
            in
            case findByIdWithIndex id list of
                Just ( idx, item ) ->
                    applyNestedInsertRow lens.nested rest index json item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Field _) :: _ ->
            Err "Unexpected field segment at list level"


moveRowInListField : ListLens record item -> List PathSegment -> Int -> Int -> record -> Result String record
moveRowInListField lens segments from to record =
    case segments of
        [] ->
            -- Move at this list level
            Ok (lens.set (moveInList from to (lens.get record)) record)

        (Index idx) :: rest ->
            let
                list =
                    lens.get record
            in
            case getAtIndex idx list of
                Just item ->
                    applyNestedMoveRow lens.nested rest from to item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id id) :: rest ->
            let
                list =
                    lens.get record
            in
            case findByIdWithIndex id list of
                Just ( idx, item ) ->
                    applyNestedMoveRow lens.nested rest from to item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Field _) :: _ ->
            Err "Unexpected field segment at list level"


removeByIndexFromList : ListLens record item -> List PathSegment -> Int -> record -> Result String record
removeByIndexFromList lens segments index record =
    case segments of
        [] ->
            -- Remove at this list level
            Ok (lens.set (removeAtIndex index (lens.get record)) record)

        (Index idx) :: rest ->
            let
                list =
                    lens.get record
            in
            case getAtIndex idx list of
                Just item ->
                    applyNestedRemoveByIndex lens.nested rest index item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Index out of bounds: " ++ String.fromInt idx)

        (Id id) :: rest ->
            let
                list =
                    lens.get record
            in
            case findByIdWithIndex id list of
                Just ( idx, item ) ->
                    applyNestedRemoveByIndex lens.nested rest index item
                        |> Result.map (\newItem -> lens.set (setAtIndex idx newItem list) record)

                Nothing ->
                    Err ("Id not found: " ++ id)

        (Field _) :: _ ->
            Err "Unexpected field segment at list level"


-- MAYBE FIELD OPERATIONS


setRowInMaybe : MaybeLens record item -> List PathSegment -> Decode.Value -> record -> Result String record
setRowInMaybe lens segments json record =
    case segments of
        [] ->
            -- Set the Maybe value directly
            case Decode.decodeValue lens.decode json of
                Ok row ->
                    Ok (lens.set (Just row) record)

                Err err ->
                    Err ("Failed to decode row: " ++ Decode.errorToString err)

        _ ->
            -- Navigate into the Maybe value
            case lens.get record of
                Just item ->
                    applyNestedSetRow lens.nested segments json item
                        |> Result.map (\newItem -> lens.set (Just newItem) record)

                Nothing ->
                    Err "Cannot navigate into Nothing"


removeRowFromMaybe : MaybeLens record item -> List PathSegment -> record -> Result String record
removeRowFromMaybe lens segments record =
    case segments of
        [] ->
            -- Remove the Maybe value (set to Nothing)
            Ok (lens.set Nothing record)

        _ ->
            -- Navigate into the Maybe value
            case lens.get record of
                Just item ->
                    applyNestedRemoveRow lens.nested segments item
                        |> Result.map (\newItem -> lens.set (Just newItem) record)

                Nothing ->
                    Err "Cannot navigate into Nothing"


-- NESTED FIELD DISPATCH


applyNestedSetRow : (String -> Maybe (FieldHandler item)) -> List PathSegment -> Decode.Value -> item -> Result String item
applyNestedSetRow nestedLookup segments json item =
    case segments of
        (Field fieldName) :: rest ->
            case nestedLookup fieldName of
                Just (ListHandler handler) ->
                    handler.setRow rest json item

                Just (MaybeHandler handler) ->
                    handler.setRow rest json item

                Nothing ->
                    Err ("Unknown nested field: " ++ fieldName)

        _ ->
            Err "Expected field segment for nested access"


applyNestedRemoveRow : (String -> Maybe (FieldHandler item)) -> List PathSegment -> item -> Result String item
applyNestedRemoveRow nestedLookup segments item =
    case segments of
        (Field fieldName) :: rest ->
            case nestedLookup fieldName of
                Just (ListHandler handler) ->
                    handler.removeRow rest item

                Just (MaybeHandler handler) ->
                    handler.removeRow rest item

                Nothing ->
                    Err ("Unknown nested field: " ++ fieldName)

        _ ->
            Err "Expected field segment for nested access"


applyNestedInsertRow : (String -> Maybe (FieldHandler item)) -> List PathSegment -> Int -> Decode.Value -> item -> Result String item
applyNestedInsertRow nestedLookup segments index json item =
    case segments of
        (Field fieldName) :: rest ->
            case nestedLookup fieldName of
                Just (ListHandler handler) ->
                    handler.insertRow rest index json item

                Just (MaybeHandler _) ->
                    Err "Cannot insert into Maybe field"

                Nothing ->
                    Err ("Unknown nested field: " ++ fieldName)

        _ ->
            Err "Expected field segment for nested access"


applyNestedMoveRow : (String -> Maybe (FieldHandler item)) -> List PathSegment -> Int -> Int -> item -> Result String item
applyNestedMoveRow nestedLookup segments from to item =
    case segments of
        (Field fieldName) :: rest ->
            case nestedLookup fieldName of
                Just (ListHandler handler) ->
                    handler.moveRow rest from to item

                Just (MaybeHandler _) ->
                    Err "Cannot move in Maybe field"

                Nothing ->
                    Err ("Unknown nested field: " ++ fieldName)

        _ ->
            Err "Expected field segment for nested access"


applyNestedRemoveByIndex : (String -> Maybe (FieldHandler item)) -> List PathSegment -> Int -> item -> Result String item
applyNestedRemoveByIndex nestedLookup segments index item =
    case segments of
        (Field fieldName) :: rest ->
            case nestedLookup fieldName of
                Just (ListHandler handler) ->
                    handler.removeByIndex rest index item

                Just (MaybeHandler _) ->
                    Err "Cannot remove by index from Maybe field"

                Nothing ->
                    Err ("Unknown nested field: " ++ fieldName)

        _ ->
            Err "Expected field segment for nested access"


-- ID LOOKUP HELPER


{-| Find an item by id in a list. Returns the index and item.
Note: This is a placeholder - actual id lookup requires type-specific code.
-}
findByIdWithIndex : String -> List a -> Maybe ( Int, a )
findByIdWithIndex targetId list =
    findByIdWithIndexHelp targetId 0 list


findByIdWithIndexHelp : String -> Int -> List a -> Maybe ( Int, a )
findByIdWithIndexHelp targetId idx list =
    case list of
        [] ->
            Nothing

        _ :: rest ->
            -- Cannot generically access .id in Elm, so id selectors
            -- need Index paths from the server instead
            findByIdWithIndexHelp targetId (idx + 1) rest


-- GENERIC APPLY


{-| Apply a list of delta operations to a record using field handlers.
-}
applyOps : List ( String, FieldHandler record ) -> List DeltaOp -> record -> Result String record
applyOps fields ops data =
    List.foldl
        (\op acc ->
            Result.andThen (applyOp fields op) acc
        )
        (Ok data)
        ops


applyOp : List ( String, FieldHandler record ) -> DeltaOp -> record -> Result String record
applyOp fields op record =
    let
        lookupField name =
            List.filterMap
                (\( n, handler ) ->
                    if n == name then
                        Just handler

                    else
                        Nothing
                )
                fields
                |> List.head
    in
    case op of
        SetRow ( field, segments ) json ->
            case lookupField field of
                Just (ListHandler handler) ->
                    handler.setRow segments json record

                Just (MaybeHandler handler) ->
                    handler.setRow segments json record

                Nothing ->
                    Err ("Unknown field: " ++ field)

        RemoveRow ( field, segments ) ->
            case lookupField field of
                Just (ListHandler handler) ->
                    handler.removeRow segments record

                Just (MaybeHandler handler) ->
                    handler.removeRow segments record

                Nothing ->
                    Err ("Unknown field: " ++ field)

        InsertRow ( field, segments ) index json ->
            case lookupField field of
                Just (ListHandler handler) ->
                    handler.insertRow segments index json record

                Just (MaybeHandler _) ->
                    Err "Cannot insert into Maybe field"

                Nothing ->
                    Err ("Unknown field: " ++ field)

        MoveRow ( field, segments ) from to ->
            case lookupField field of
                Just (ListHandler handler) ->
                    handler.moveRow segments from to record

                Just (MaybeHandler _) ->
                    Err "Cannot move in Maybe field"

                Nothing ->
                    Err ("Unknown field: " ++ field)

        RemoveRowByIndex ( field, segments ) index ->
            case lookupField field of
                Just (ListHandler handler) ->
                    handler.removeByIndex segments index record

                Just (MaybeHandler _) ->
                    Err "Cannot remove by index from Maybe field"

                Nothing ->
                    Err ("Unknown field: " ++ field)
