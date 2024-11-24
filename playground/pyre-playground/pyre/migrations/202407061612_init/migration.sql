create table "users" (
    "id" integer not null primary key autoincrement,
    "name" text not null,
    "status" text not null, -- Status,
    "createdAt" integer not null default (unixepoch())

);
create table "accounts" (
    "id" integer not null primary key autoincrement,
    "userId" integer not null,
    "name" text not null,
    "status" text not null,
    constraint "accounts_userId_User_id_fk" foreign key ("userId") references "users" ("id")
);
create table "jobs" (
    "id" integer not null primary key autoincrement,
    "name" text not null

);
create table "posts" (
    "id" integer not null primary key autoincrement,
    "createdAt" integer not null default (unixepoch()),
    "authorUserId" integer not null,
    "title" text not null,
    "content" text not null,
    "status" text not null, -- Status,
    constraint "posts_authorUserId_User_id_fk" foreign key ("authorUserId") references "users" ("id")
);
