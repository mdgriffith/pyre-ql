create table "games" (
    "id" integer not null primary key autoincrement,
    "name" text not null

);
create table "players" (
    "id" integer not null primary key autoincrement,
    "userId" integer not null,
    "name" text not null,
    "points" integer not null,
    "gameId" integer not null,
    constraint "players_userId_User_id_fk" foreign key ("userId") references "User" ("id"),
    constraint "players_gameId_Game_id_fk" foreign key ("gameId") references "games" ("id")
);
create table "jobs" (
    "id" integer not null primary key autoincrement,
    "name" text not null

);