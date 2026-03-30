module SyncStateTest exposing (suite)

import Data.SyncState as SyncState
import Dict
import Expect
import Test exposing (Test, describe, test)


suite : Test
suite =
    describe "SyncState"
        [ test "initialTableStatuses sets all tables to waiting" <|
            \_ ->
                let
                    tables =
                        Dict.fromList
                            [ ( "users", () )
                            , ( "posts", () )
                            ]

                    statuses =
                        SyncState.initialTableStatuses tables
                in
                Expect.equal
                    (Dict.fromList
                        [ ( "users", SyncState.Waiting )
                        , ( "posts", SyncState.Waiting )
                        ]
                    )
                    statuses
        , test "markTablesCatchingUp only updates requested tables" <|
            \_ ->
                let
                    statuses =
                        Dict.fromList
                            [ ( "users", SyncState.Waiting )
                            , ( "posts", SyncState.Waiting )
                            ]

                    updated =
                        SyncState.markTablesCatchingUp [ "posts" ] statuses
                in
                Expect.equal
                    (Dict.fromList
                        [ ( "users", SyncState.Waiting )
                        , ( "posts", SyncState.TableCatchingUp )
                        ]
                    )
                    updated
        , test "markAllTablesLive sets all tables to live" <|
            \_ ->
                let
                    statuses =
                        Dict.fromList
                            [ ( "users", SyncState.Waiting )
                            , ( "posts", SyncState.TableCatchingUp )
                            ]

                    updated =
                        SyncState.markAllTablesLive statuses
                in
                Expect.equal
                    (Dict.fromList
                        [ ( "users", SyncState.TableLive )
                        , ( "posts", SyncState.TableLive )
                        ]
                    )
                    updated
        ]
