create table "users" (
    "id" integer not null primary key autoincrement,
    "name" text,
    "status" text not null, -- Status,
    "createdAt" integer not null default (unixepoch())

);
create table "databaseUsers" (
    "id" integer not null primary key autoincrement,
    "databaseId" text not null,
    "userId" integer not null,
    constraint "databaseUsers_userId_User_id_fk" foreign key ("userId") references "users" ("id")
);
create table "accounts" (
    "id" integer not null primary key autoincrement,
    "userId" integer not null,
    "name" text not null,
    "status" text not null,
    constraint "accounts_userId_User_id_fk" foreign key ("userId") references "users" ("id")
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
