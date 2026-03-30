module Data.SyncState exposing (SyncState, SyncStatus(..), TableSyncStatus(..), encodeSyncState, initialTableStatuses, markAllTablesLive, markTablesCatchingUp)

import Dict exposing (Dict)
import Json.Encode as Encode

type SyncStatus = NotStarted | CatchingUp | Live

type TableSyncStatus = Waiting | TableCatchingUp | TableLive

type alias SyncState =
    { status : SyncStatus
    , tables : Dict String TableSyncStatus
    }

initialTableStatuses : Dict String tableMetadata -> Dict String TableSyncStatus
initialTableStatuses tables =
    Dict.map (\_ _ -> Waiting) tables

markTablesCatchingUp : List String -> Dict String TableSyncStatus -> Dict String TableSyncStatus
markTablesCatchingUp tableNames statuses =
    List.foldl
        (\tableName acc ->
            case Dict.get tableName acc of
                Just TableLive ->
                    acc

                Just _ ->
                    Dict.insert tableName TableCatchingUp acc

                Nothing ->
                    acc
        )
        statuses
        tableNames

markAllTablesLive : Dict String TableSyncStatus -> Dict String TableSyncStatus
markAllTablesLive =
    Dict.map (\_ _ -> TableLive)

encodeSyncState : SyncState -> Encode.Value
encodeSyncState syncState =
    Encode.object
        [ ( "status", encodeSyncStatus syncState.status )
        , ( "tables", Encode.dict identity encodeTableSyncStatus syncState.tables )
        ]

encodeSyncStatus : SyncStatus -> Encode.Value
encodeSyncStatus status =
    case status of
        NotStarted ->
            Encode.string "not_started"

        CatchingUp ->
            Encode.string "catching_up"

        Live ->
            Encode.string "live"

encodeTableSyncStatus : TableSyncStatus -> Encode.Value
encodeTableSyncStatus status =
    case status of
        Waiting ->
            Encode.string "waiting"

        TableCatchingUp ->
            Encode.string "catching_up"

        TableLive ->
            Encode.string "live"
