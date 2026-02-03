create table "posts" (
  `id` INTEGER primary key autoincrement not null,
  `authorUserId` INTEGER not null,
  `title` TEXT not null,
  `content` TEXT not null,
  `published` INTEGER not null default (0),
  `createdAt` INTEGER not null default (unixepoch()),
  `updatedAt` INTEGER not null default (unixepoch())
);
create index if not exists "idx_posts_updatedAt" on "posts" ("updatedAt");
create table "users" (
  `id` INTEGER primary key autoincrement not null,
  `name` TEXT not null,
  `email` TEXT not null,
  `createdAt` INTEGER not null default (unixepoch()),
  `updatedAt` INTEGER not null default (unixepoch())
);
create index if not exists "idx_users_updatedAt" on "users" ("updatedAt");
