use crate::ast;

use crate::generate::sql::to_sql;
use crate::typecheck;
use std::collections::HashSet;

/*

So, there's a potentially spicy decision to have SQLite handle composing the JSON for us.

This dramatically reduces the generated client code.


// Query

    query Games {
        game {
            @where { ownerId = Session.userId }
            id
            name
            myPlayersAlias: players {
                id
                aliasedName: name
                points
                user {
                    id
                    name
                    posts {
                        title
                        content
                    }
                }
            }
        }
    }


// Example SQL
//

    WITH
    GamesFilter AS (
        SELECT id, name
        FROM games
        WHERE owner_id = :userId
    ),
    PlayersForGames AS (
        SELECT
            game_id,
            JSON_GROUP_ARRAY(
                JSON_OBJECT(
                    'id', id,
                    'aliasedName', name,
                    'points', points
                )
            ) AS players_json
        FROM players
        WHERE game_id IN (SELECT id FROM GamesFilter)
        GROUP BY game_id
    )

    SELECT
        JSON_OBJECT(
            'game', JSON_GROUP_ARRAY(
                JSON_OBJECT(
                    'id', g.id,
                    'name', g.name,
                    'players', COALESCE(p.players_json, '[]')
                )
            )
        ) AS result
    FROM GamesFilter g
    LEFT JOIN PlayersForGames p ON g.id = p.game_id;






*/

pub fn select_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
) -> Vec<to_sql::Prepared> {
    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut last_link_index = 0;
    for (i, query_field) in all_query_fields.iter().enumerate() {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(_)) => {
                last_link_index = i;
            }
            _ => (),
        }
    }

    let initial_indent = if last_link_index == 0 { 0 } else { 4 };
    let mut statements = to_sql::format_attach(query_info);

    let mut result = String::new();

    let initial_selection =
        initial_select(initial_indent, context, query, table, query_table_field);
    let parent_table_alias = &get_temp_table_name(&query_table_field);

    let mut rendered_initial = false;

    for query_field in all_query_fields.iter() {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                // We are inserting a link, so we need to do a nested insert
                let linked_table = typecheck::get_linked_table(context, &link).unwrap();

                if !rendered_initial {
                    result.push_str("with ");
                    result.push_str(parent_table_alias);
                    result.push_str(" as (\n");
                    result.push_str(&initial_selection);
                    result.push_str("\n)");
                    rendered_initial = true;
                }

                let temp_table_alias = &get_temp_table_name(&query_field);

                result.push_str(",");
                result.push_str(" ");
                result.push_str(temp_table_alias);
                result.push_str(" as (");

                select_linked(
                    4,
                    context,
                    query,
                    parent_table_alias,
                    linked_table,
                    query_field,
                    link,
                    &mut result,
                );

                result.push_str(")");
            }
            _ => (),
        }
    }

    // The final selection
    final_select_formatted_as_json(
        0,
        context,
        query,
        parent_table_alias,
        table,
        query_table_field,
        &mut result,
    );

    statements.push(to_sql::include(result));
    statements
}

pub fn get_temp_table_name(query_field: &ast::QueryField) -> String {
    format!("selected__{}", &ast::get_aliased_name(&query_field))
}

pub fn get_json_temp_table_name(query_field: &ast::QueryField) -> String {
    format!("json__{}", &ast::get_aliased_name(&query_field))
}

pub fn initial_select(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
) -> String {
    let indent_str = " ".repeat(indent);
    let mut field_names: Vec<String> = Vec::new();

    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &query_table_field.fields,
    );
    field_names.append(&mut new_fieldnames.clone());

    format!(
        "{}select {}\n{}from {}",
        indent_str,
        field_names.join(", "),
        indent_str,
        table_name,
    )
}

fn select_linked(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,

    //
    sql: &mut String,
) {
    /* This funciton can produce a few different sql forms


    1. If we're selecting through any links

        -- initial selection of data
        selected_myPlayersAlias as (
            select gameId, id, name, points
            from players
            where gameId in (select id from selected_game)
        )

        --> recursively select_linked data

        -- Formatted as JSON
        json_myPlayersAlias AS (
            select
                game_id,
                json_group_array(
                    json_object(
                        'id', selected_myPlayersAlias.player_id,
                        'aliasedName', selected_myPlayersAlias.player_name,
                        'points', selected_myPlayersAlias.player_points,
                        'user', COALESCE(uf.user_json, '{}')
                    )
                ) AS players_json
            from selected_myPlayersAlias selected_myPlayersAlias
            // Join children data
            left join UsersForPlayers uf ON selected_myPlayersAlias.player_id = uf.player_id
            group by game_id
        )


    2. If there are *NO* links, we select the full selection as json

        selected_myPlayersAlias as (
            select
                gameId,
                JSON_OBJECT(
                    'id', id,
                    'name', name,
                    'points', points
                )
            from players
            where gameId in (select id from selected_game)
            group
        )

        There is no need to `group`` or `json_group_array` in this case.






     */

    if !selects_for_link(query_table_field, table) {
        select_single_json(
            indent,
            context,
            query,
            parent_table_name,
            table,
            query_table_field,
            link,
            sql,
        );
        return;
    }

    let indent_str = " ".repeat(indent);

    let mut field_names: Vec<String> = Vec::new();
    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &query_table_field.fields,
    );
    field_names.push(link.foreign.fields.clone().join(", "));
    field_names.append(&mut new_fieldnames.clone());
    // field_names.push

    // initial selection
    sql.push_str(&format!(
        "\n{}select {}\n{}from {}\n",
        indent_str,
        field_names.join(", "),
        indent_str,
        table_name,
    ));

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut full_local_id = String::new();
    for local_id in &link.local_ids {
        full_local_id.push_str(local_id);
    }

    // This is the link.foreign_id, which is the id on this table
    let mut full_foreign_id = String::new();
    for foreign_id in &link.foreign.fields {
        full_foreign_id.push_str(foreign_id);
    }

    sql.push_str(&format!(
        "{}where {} in (select {} from {})\n",
        indent_str, full_foreign_id, full_local_id, parent_table_name
    ));
    // result.push_str(&format!("{}from {}", indent_str, parent_table_name));

    // result.push_str(&format!("{}group by {}", indent_str, full_foreign_id));

    // Recursively define children
    let parent_temp_table = &get_temp_table_name(&query_table_field);

    for query_field in all_query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        let temp_table_name = &get_temp_table_name(&query_field);

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                // We are inserting a link, so we need to do a nested insert
                let linked_table = typecheck::get_linked_table(context, &link).unwrap();

                sql.push_str(&format!("), {} as (", temp_table_name));
                select_linked(
                    indent,
                    context,
                    query,
                    parent_temp_table,
                    linked_table,
                    query_field,
                    &link,
                    sql,
                );
            }
            _ => (),
        }
    }

    let json_table_name = &get_json_temp_table_name(&query_table_field);

    sql.push_str(&format!("), {} as (", json_table_name));
    // Format as JSON
    select_formatted_as_json(
        indent,
        context,
        query,
        parent_table_name,
        table,
        query_table_field,
        link,
        sql,
    );
}

fn selects_for_link(query: &ast::QueryField, table: &typecheck::Table) -> bool {
    for query_field in &query.fields {
        match query_field {
            ast::ArgField::Field(qf) => {
                let link_present = table
                    .record
                    .fields
                    .iter()
                    .any(|f| ast::has_link_named(&f, &qf.name));

                if link_present {
                    return true;
                }
            }
            _ => continue,
        }
    }
    return false;
}

/*

A simple as possible json selection

     selected_myPlayersAlias as (
        select
            gameId,
            JSON_OBJECT(
                'id', id,
                'name', name,
                'points', points
            )
        from players
        where gameId in (select id from selected_game)
        group
    )



*/
fn select_single_json(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,

    //
    sql: &mut String,
) {
    let indent_str = " ".repeat(indent);

    let aggregate_to_array = !ast::linked_to_unique_field(link);

    // This is the link.foreign_id, which is the id on this table
    let mut full_foreign_id = String::new();
    for foreign_id in &link.foreign.fields {
        full_foreign_id.push_str(foreign_id);
    }

    let query_aliased_as = &ast::get_aliased_name(&query_table_field);

    // Compose main json payload
    let mut json_object = String::new();
    let new_fieldnames =
        &to_fieldnames(context, query_aliased_as, table, &query_table_field.fields);
    let mut first_field = true;
    for field in new_fieldnames {
        if !first_field {
            json_object.push_str(",\n");
        }
        json_object.push_str(&format!("{}    '{}', {}", indent_str, field, field));
        first_field = false;
    }

    // initial selection
    #[rustfmt::skip]
    let array_agg_start = if aggregate_to_array  { "jsonb_group_array(" } else { "" };
    #[rustfmt::skip]
    let array_agg_end = if aggregate_to_array { ")" } else { "" };

    sql.push_str(&format!(
        "\n{}select\n  {}{},\n{}  {}jsonb_object(\n{}\n{}  ){} as {}\n{}from {}\n",
        indent_str,
        indent_str,
        full_foreign_id,
        indent_str,
        array_agg_start,
        json_object,
        indent_str,
        array_agg_end,
        query_aliased_as,
        indent_str,
        ast::get_tablename(&table.record.name, &table.record.fields),
    ));

    let mut full_local_id = String::new();
    for local_id in &link.local_ids {
        full_local_id.push_str(local_id);
    }

    sql.push_str(&format!(
        "{}where {} in (select {} from {})\n",
        indent_str, full_foreign_id, full_local_id, parent_table_name
    ));

    if aggregate_to_array {
        sql.push_str(&format!("{}group by {}\n", indent_str, full_foreign_id));
    }
}

/*

We're formatting a grouped table

     selected_myPlayersAlias as (
        select
            gameId,
            JSON_OBJECT(
                'id', id,
                'name', name,
                'points', points
            )
        from players
        where gameId in (select id from selected_game)
        group
    )

    json_myPlayersAlias AS (
        SELECT
            game_id,
            JSON_GROUP_ARRAY(
                JSON_OBJECT(
                    'id', pf.player_id,
                    'aliasedName', pf.player_name,
                    'points', pf.player_points,
                    'user', COALESCE(uf.user_json, '{}')
                )
            ) AS players_json
        FROM PlayersForGames pf
        LEFT JOIN UsersForPlayers uf ON pf.player_id = uf.player_id
        GROUP BY game_id
)



*/
fn select_formatted_as_json(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,

    //
    sql: &mut String,
) {
    let indent_str = " ".repeat(indent);

    let aliased_name = ast::get_aliased_name(query_table_field);
    let base_table_name = format!("selected__{}", &aliased_name);

    // This is the link.foreign_id, which is the id on this table
    let mut full_foreign_id = String::new();
    for foreign_id in &link.foreign.fields {
        full_foreign_id.push_str(foreign_id);
    }

    // initial selection
    sql.push_str(&format!(
        "\n{}select\n  {}{}.{},\n{}  jsonb_group_array(jsonb_object(\n",
        indent_str, indent_str, base_table_name, full_foreign_id, indent_str
    ));

    // Compose main json payload
    let mut first_field = true;
    for field in &query_table_field.fields {
        match field {
            ast::ArgField::Field(query_field) => {
                let table_field = table
                    .record
                    .fields
                    .iter()
                    .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    .unwrap();

                let aliased_field_name = ast::get_aliased_name(query_field);

                match table_field {
                    ast::Field::Column(_) => {
                        if !first_field {
                            sql.push_str(",\n");
                        }
                        sql.push_str(&format!(
                            "{}    '{}', {}.{}",
                            indent_str, aliased_field_name, base_table_name, query_field.name
                        ));
                        first_field = false;
                    }
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        if !first_field {
                            sql.push_str(",\n");
                        }
                        if ast::linked_to_unique_field(link) {
                            // singular result, no need to coalesce

                            sql.push_str(&format!(
                                "{}    '{}', temp__{}.{}",
                                indent_str,
                                aliased_field_name,
                                query_field.name,
                                aliased_field_name,
                            ));
                        } else {
                            // Coalesce as an empty array
                            sql.push_str(&format!(
                                "{}    '{}', coalesce(temp__{}.{}, jsonb('[]'))",
                                indent_str,
                                aliased_field_name,
                                query_field.name,
                                aliased_field_name,
                            ));
                        }
                        first_field = false;
                    }
                    _ => continue,
                }
            }
            ast::ArgField::Arg(_)
            | ast::ArgField::Lines { .. }
            | ast::ArgField::QueryComment { .. } => continue,
        }
    }

    sql.push_str(&format!("\n{}  )) as {}\n", indent_str, aliased_name));

    // FROM
    sql.push_str(&format!("{}from {}\n", indent_str, base_table_name));

    // Join every link
    for field in &query_table_field.fields {
        match field {
            ast::ArgField::Field(query_field) => {
                let table_field = table
                    .record
                    .fields
                    .iter()
                    .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    .unwrap();

                match table_field {
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        let linked_table = typecheck::get_linked_table(context, &link).unwrap();

                        let query_temp_table = to_temp_table_alias(query_field, linked_table);
                        let local_temp_alias = format!("temp__{}", link.link_name);

                        sql.push_str(&format!(
                            "{}  left join {} {} on {}.{} = {}.{}\n",
                            indent_str,
                            query_temp_table,
                            local_temp_alias,
                            local_temp_alias,
                            link.foreign.fields.join(""),
                            base_table_name,
                            link.local_ids.join(""),
                        ));
                    }
                    _ => continue,
                }
            }
            ast::ArgField::Arg(_)
            | ast::ArgField::Lines { .. }
            | ast::ArgField::QueryComment { .. } => continue,
        }
    }

    sql.push_str(&format!("{}group by {}\n", indent_str, full_foreign_id));
}

/*
This is slightly different than the others.

1. it should use `json`, not jsonb, because it's returning a final result



*/
fn final_select_formatted_as_json(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,

    //
    sql: &mut String,
) {
    let indent_str = " ".repeat(indent);

    let aliased_name = ast::get_aliased_name(query_table_field);
    let base_table_name = format!("selected__{}", &aliased_name);

    // initial selection
    sql.push_str(&format!("\nselect\n{}  json_object(\n", indent_str));

    // Compose main json payload
    let mut first_field = true;
    for field in &query_table_field.fields {
        match field {
            ast::ArgField::Field(query_field) => {
                let table_field = table
                    .record
                    .fields
                    .iter()
                    .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    .unwrap();

                let aliased_field_name = ast::get_aliased_name(query_field);

                match table_field {
                    ast::Field::Column(_) => {
                        if !first_field {
                            sql.push_str(",\n");
                        }
                        // sql.push_str(&format!(
                        //     "{}    {}.{} as {}",
                        //     indent_str, base_table_name, query_field.name, aliased_field_name
                        // ));
                        sql.push_str(&format!(
                            "{}    '{}', {}.{}",
                            indent_str, aliased_field_name, base_table_name, query_field.name
                        ));
                        first_field = false;
                    }
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        if !first_field {
                            sql.push_str(",\n");
                        }

                        if ast::linked_to_unique_field(link) {
                            // singular result, no need to coalesce
                            sql.push_str(&format!(
                                "{}    '{}', temp__{}.{}",
                                indent_str,
                                aliased_field_name,
                                query_field.name,
                                aliased_field_name
                            ));
                            first_field = false;
                        } else {
                            sql.push_str(&format!(
                                "{}    '{}', coalesce(temp__{}.{}, jsonb('[]'))",
                                indent_str,
                                aliased_field_name,
                                query_field.name,
                                aliased_field_name
                            ));
                            first_field = false;
                        }
                    }
                    _ => continue,
                }
            }
            ast::ArgField::Arg(_)
            | ast::ArgField::Lines { .. }
            | ast::ArgField::QueryComment { .. } => continue,
        }
    }

    // FROM
    sql.push_str(&format!(
        "\n  ) as {}\n{}from {}\n",
        aliased_name, indent_str, base_table_name
    ));

    // Join every link
    for field in &query_table_field.fields {
        match field {
            ast::ArgField::Field(query_field) => {
                let table_field = table
                    .record
                    .fields
                    .iter()
                    .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    .unwrap();

                match table_field {
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        // let str = link.local_ids.join("");
                        // return vec![str];

                        // let query_json_table =
                        // format!("json__{}", ast::get_aliased_name(query_field));

                        let linked_table = typecheck::get_linked_table(context, &link).unwrap();
                        let query_temp_table = to_temp_table_alias(query_field, linked_table);

                        let local_temp_alias = format!("temp__{}", link.link_name);

                        sql.push_str(&format!(
                            "{}  left join {} {} on {}.{} = {}.{}\n",
                            indent_str,
                            query_temp_table,
                            local_temp_alias,
                            local_temp_alias,
                            link.foreign.fields.join(""),
                            base_table_name,
                            link.local_ids.join(""),
                        ));
                    }
                    _ => continue,
                }
            }
            ast::ArgField::Arg(_)
            | ast::ArgField::Lines { .. }
            | ast::ArgField::QueryComment { .. } => continue,
        }
    }

    // sql.push_str(&format!("{}group by {}\n", indent_str, full_foreign_id));
}

fn to_temp_table_alias(query_field: &ast::QueryField, table: &typecheck::Table) -> String {
    let query_alias = ast::get_aliased_name(query_field);

    if selects_for_link(query_field, table) {
        return format!("json__{}", query_alias);
    } else {
        return format!("selected__{}", query_alias);
    }
}

// Field names

fn to_fieldnames(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    query_fields: &Vec<ast::ArgField>,
) -> Vec<String> {
    let mut result = vec![];
    let mut selected_set = HashSet::new();

    for field in query_fields {
        match field {
            ast::ArgField::Field(query_field) => {
                let table_field = table
                    .record
                    .fields
                    .iter()
                    .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
                    .unwrap();

                match to_table_fieldname(table_field, query_field) {
                    Some(selected_fieldname) => {
                        if !selected_set.contains(&selected_fieldname) {
                            selected_set.insert(selected_fieldname.clone());
                            result.push(selected_fieldname);
                        }
                    }
                    None => (),
                }
            }
            ast::ArgField::Arg(_)
            | ast::ArgField::Lines { .. }
            | ast::ArgField::QueryComment { .. } => continue,
        }
    }

    result
}

fn to_table_fieldname(table_field: &ast::Field, query_field: &ast::QueryField) -> Option<String> {
    match table_field {
        ast::Field::Column(_) => {
            let str = query_field.name.to_string();
            return Some(str);
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let str = link.local_ids.join("");
            return Some(str);
        }
        _ => None,
    }
}

/*
Querying things and then incrementally encoding

WITH
GamesFilter AS (
    SELECT id, name
    FROM games
    WHERE id = 1
),
PlayersForGames AS (
    SELECT
        p.game_id,
        p.id AS player_id,
        p.name AS player_name,
        p.points AS player_points,
        p.user_id
    FROM players p
    WHERE p.game_id IN (SELECT id FROM GamesFilter)
),
UsersForPlayers AS (
    SELECT
        pf.player_id,
        JSON_OBJECT(
            'userName', u.name
        ) AS user_json
    FROM PlayersForPlayers pf
    LEFT JOIN users u ON pf.user_id = u.id
),
FormattedPlayersWithUsers AS (
    SELECT
        game_id,
        JSON_GROUP_ARRAY(
            JSON_OBJECT(
                'id', pf.player_id,
                'aliasedName', pf.player_name,
                'points', pf.player_points,
                'user', COALESCE(uf.user_json, '{}')
            )
        ) AS players_json
    FROM PlayersForGames pf
    LEFT JOIN UsersForPlayers uf ON pf.player_id = uf.player_id
    GROUP BY game_id
)

SELECT
    JSON_OBJECT(
        'game', JSON_GROUP_ARRAY(
            JSON_OBJECT(
                'id', g.id,
                'name', g.name,
                'players', COALESCE(p.players_json, '[]')
            )
        )
    ) AS result
FROM GamesFilter g
LEFT JOIN FormattedPlayersWithUsers p ON g.id = p.game_id;



*/
