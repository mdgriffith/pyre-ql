/*


Given a query, we have 3 choices for generating sql.
1. Normal: A normal join
2. Batch: Flatten and batch the queries
3. CTE: Use a CTE (Common Table Expression, a temporary table)

Batches are basically like a CTE, but where we have to do the join in the application layer.

So, our first approach is going to be using a CTE.

For selects, here's how we choose what strategy to take.

1. We default to using the join.
2. If there is a limit/offset, we use the CTE form.
3. If there is a @where on anything but the top-level table, we need to use a CTE


2 is because the limit applies to the result, but conceptually we want it to apply to the table it's attached to.
So, if we add an @limit 1 to our query for users and their blogposts, we will only return 1 user and maybe 1 blogpost.
And if the limit is 2, we could return 1-2 users and 1-2 blogposts.

With 'where' it's the same conceptual problem.



Exmple query Generation:

    query Init($userId: Int) {
        user {
            @limit 1
            @where { id = $userId }
            id
            name
            email
            games: gameMembers {
                admin
                game {
                    id
                    name
                }
            }
        }
    }

SQL, CTE Form

    WITH user_info AS (
        SELECT
            user.id AS user_id,
            user.name AS user_name,
            user.email AS user_email
        FROM user
        WHERE user.id = $userId
        LIMIT 1
    ),
    game_members_info AS (
        SELECT
            gameMembers.user_id AS gm_user_id,
            gameMembers.admin AS gm_admin,
            game.id AS game_id,
            game.name AS game_name
        FROM gameMembers
        JOIN game ON game.id = gameMembers.game_id
    )
    SELECT
        ui.user_id AS id,
        ui.user_name AS name,
        ui.user_email AS email,
        gmi.gm_admin AS admin,
        gmi.game_id AS "game.id",
        gmi.game_name AS "game.name"
    FROM user_info ui
    LEFT JOIN game_members_info gmi ON ui.user_id = gmi.gm_user_id;


This uses CTE (Common Table Expressions)


SQL, Join Form

    SELECT
        user.id AS id,
        user.name AS name,
        user.email AS email,
        gameMembers.admin AS admin,
        game.id AS "game.id",
        game.name AS "game.name"
    FROM user
    LEFT JOIN gameMembers ON user.id = gameMembers.user_id
    LEFT JOIN game ON game.id = gameMembers.game_id
    WHERE user.id = $userId
    LIMIT 1;

Subselect for

    SELECT
        ui.user_id AS id,
        ui.user_name AS name,
        ui.user_email AS email,
        gmi.gm_admin AS admin,
        gmi.game_id AS "game.id",
        gmi.game_name AS "game.name"
    FROM (
        SELECT
            user.id AS user_id,
            user.name AS user_name,
            user.email AS user_email
        FROM user
        WHERE user.id = $userId
        LIMIT 1
    ) AS ui
    LEFT JOIN (
        SELECT
            gameMembers.user_id AS gm_user_id,
            gameMembers.admin AS gm_admin,
            game.id AS game_id,
            game.name AS game_name
        FROM gameMembers
        JOIN game ON game.id = gameMembers.game_id
    ) AS gmi ON ui.user_id = gmi.gm_user_id;


And there is also Batch form, where we execute separate sql queries.


    SELECT
        user.id AS id,
        user.name AS name,
        user.email AS email
    FROM user
    WHERE user.id = $userId
    LIMIT 1;

    SELECT
        gameMembers.admin AS admin,
        game.id AS "game.id",
        game.name AS "game.name"
    FROM gameMembers
    JOIN game ON game.id = gameMembers.game_id
    WHERE gameMembers.user_id = $userId;




We generally only need to use something other than the standard join approach when there is
a limit/offset. In that case, we need to use a CTE or a batched appraoch to get the limit and then join the other tables.

*/

pub mod insert;
pub mod update;
