pub mod cte;
pub mod to_sql;

use crate::ast;
use crate::ext::string;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

/*
Exmple query Generation:

    query Init($userId: Int) {
        user {
            @limit 1
            @where id = $userId
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

//  QUERIES
//
pub fn to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_fields: &Vec<&ast::QueryField>,
) -> String {
    match query.operation {
        ast::QueryOperation::Select => select_to_string(context, query, query_fields),
        ast::QueryOperation::Insert => insert_to_string(context, query, query_fields),
        ast::QueryOperation::Update => update_to_string(context, query, query_fields),
        ast::QueryOperation::Delete => delete_to_string(context, query, query_fields),
    }
}

pub fn insert_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_fields: &Vec<&ast::QueryField>,
) -> String {
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);
    let mut field_names: Vec<String> = Vec::new();
    let mut table_name = query.name.clone();
    for table_field in query_fields {
        let table = context.tables.get(&table_field.name).unwrap();

        let table_name = ast::get_tablename(&table.name, &table.fields);
        let mut new_fieldnames = &to_fieldnames(
            context,
            &ast::get_aliased_name(&table_field),
            table,
            &ast::collect_query_fields(&table_field.fields),
        );
        field_names.append(&mut new_fieldnames.clone());
    }

    let mut result = format!("insert into {} ({})\n", table_name, field_names.join(", "));

    let mut values: Vec<String> = Vec::new();
    for table_field in query_fields {
        let table = context.tables.get(&table_field.name).unwrap();
        let mut new_values = &to_field_insert_values(
            context,
            &ast::get_aliased_name(&table_field),
            table,
            &ast::collect_query_fields(&table_field.fields),
        );
        values.append(&mut new_values.clone());
    }

    result.push_str(&format!("values ({});", values.join(", ")));
    result
}

pub fn update_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_fields: &Vec<&ast::QueryField>,
) -> String {
    let mut table_name = query.name.clone();
    for table_field in query_fields {
        let table = context.tables.get(&table_field.name).unwrap();

        table_name = ast::get_tablename(&table.name, &table.fields);
    }
    let mut result = format!("update {}\n", table_name);

    // UPDATE users
    // SET credit = 150
    // WHERE username = 'john_doe';
    //

    let mut values: Vec<String> = Vec::new();
    for table_field in query_fields {
        let table = context.tables.get(&table_field.name).unwrap();
        let mut new_values = &to_field_set_values(
            context,
            &ast::get_aliased_name(&table_field),
            table,
            &ast::collect_query_fields(&table_field.fields),
        );
        values.append(&mut new_values.clone());
    }

    result.push_str(&format!("set {}", values.join(", ")));

    result.push_str("\n");
    render_where(context, query_fields, &mut result);
    result.push_str(";");
    result
}

pub fn delete_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_fields: &Vec<&ast::QueryField>,
) -> String {
    let mut table_name = query.name.clone();
    for table_field in query_fields {
        let table = context.tables.get(&table_field.name).unwrap();

        table_name = ast::get_tablename(&table.name, &table.fields);
    }

    let mut result = format!("delete from {}\n", table_name);
    // DELETE FROM users
    // WHERE username = 'john_doe';

    render_where(context, query_fields, &mut result);
    result.push_str(";");

    result
}

/*

Given a query, we have 3 choices for generating sql.
1. Normal: A normal join
2. Batch: Flatten and batch the queries
3. CTE: Use a CTE

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




*/
pub fn select_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_fields: &Vec<&ast::QueryField>,
) -> String {
    if cte::should_use_cte(query) {
        let mut result = "".to_string();

        for query_top_field in query_fields {
            cte::select_to_string(context, query, query_top_field, &mut result);
        }

        return result;
    }
    let mut result = "select\n".to_string();

    // Selection
    for field in query_fields {
        let table = context.tables.get(&field.name).unwrap();
        let selected = &to_selection(
            context,
            &ast::get_aliased_name(&field),
            table,
            &ast::collect_query_fields(&field.fields),
        );
        result.push_str("  ");
        result.push_str(&selected.join(",\n  "));
        result.push_str("\n");
    }

    // FROM
    render_from(context, query_fields, &mut result);

    // WHERE
    render_where(context, query_fields, &mut result);

    // Order by
    render_order_by(context, query_fields, &mut result);

    // LIMIT
    render_limit(context, query_fields, &mut result);

    // OFFSET
    render_offset(context, query_fields, &mut result);

    result.push_str(";");

    result
}

fn render_from(
    context: &typecheck::Context,
    query_fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    result.push_str("from\n");
    for field in query_fields {
        let table = context.tables.get(&field.name).unwrap();

        let table_name = ast::get_tablename(&table.name, &table.fields);
        let mut from_vals = &mut to_from(
            context,
            &ast::get_aliased_name(&field),
            table,
            &ast::collect_query_fields(&field.fields),
        );

        // the from statements are naturally in reverse order
        // Because we're walking outwards from the root, and `.push` ing the join statements
        // Now re reverse them so they're in the correct order.
        from_vals.reverse();

        result.push_str(&format!("  {}", quote(&table_name)));
        if (from_vals.is_empty()) {
            result.push_str("\n");
        } else {
            result.push_str("\n  ");
            result.push_str(&from_vals.join("\n  "));
            result.push_str("\n");
        }
    }
}

fn render_order_by(
    context: &typecheck::Context,
    query_fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    let mut order_vals = vec![];
    for query_field in query_fields {
        let table = context.tables.get(&query_field.name).unwrap();
        let table_alias = &ast::get_aliased_name(&query_field);

        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(located_arg) => {
                    if let ast::Arg::OrderBy(dir, col) = &located_arg.arg {
                        let dir_str = ast::direction_to_string(dir);
                        order_vals.push(format!(
                            "{}.{} {}",
                            quote(table_alias),
                            quote(col),
                            dir_str
                        ));
                    }
                }
                _ => continue,
            }
        }
    }
    if (!&order_vals.is_empty()) {
        result.push_str("order by ");

        let mut first = true;

        for (i, order) in order_vals.iter().enumerate() {
            if (first) {
                result.push_str(order);
                first = false;
            } else {
                result.push_str(&format!(", {}", order));
            }
        }
    }
}

fn render_limit(
    context: &typecheck::Context,
    query_fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    for query_field in query_fields {
        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(located_arg) => {
                    if let ast::Arg::Limit(val) = &located_arg.arg {
                        result.push_str(&format!("limit {}\n", to_sql::render_value(val)));
                        break;
                    }
                }
                _ => continue,
            }
        }
    }
}

fn render_offset(
    context: &typecheck::Context,
    query_fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    for query_field in query_fields {
        for field in &query_field.fields {
            match field {
                ast::ArgField::Arg(located_arg) => {
                    if let ast::Arg::Offset(val) = &located_arg.arg {
                        result.push_str(&format!("offset {}\n", to_sql::render_value(val)));
                        break;
                    }
                }
                _ => continue,
            }
        }
    }
}

fn render_where(
    context: &typecheck::Context,
    query_fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    let mut where_vals = vec![];
    for query_field in query_fields {
        let table = context.tables.get(&query_field.name).unwrap();
        let table_alias = &ast::get_aliased_name(&query_field);
        let table_name = ast::get_tablename(&table.name, &table.fields);

        let new_params =
            render_where_params(&ast::collect_query_args(&query_field.fields), &table_name);

        where_vals.extend(new_params);

        let new_where_vals = to_where(
            context,
            &table_name,
            table_alias,
            table,
            &ast::collect_query_fields(&query_field.fields),
        );

        where_vals.extend(new_where_vals);
    }
    if (!&where_vals.is_empty()) {
        result.push_str("where\n  ");
        let mut first = true;
        for wher in &where_vals {
            if (first) {
                result.push_str(&format!("{}\n", wher));
                first = false;
            } else {
                result.push_str(&format!(" {}\n", wher));
            }
        }
    }
}
// SELECT
fn to_selection(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        let table_name = ast::get_tablename(&table.name, &table.fields);

        result.append(&mut to_subselection(
            2,
            context,
            &table_name,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subselection(
    indent: usize,
    context: &typecheck::Context,
    table_name: &str,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            let str = format!(
                "{}.{} as {}",
                format_tablename(table_name),
                quote(&query_field.name),
                quote(&ast::get_select_alias(
                    table_alias,
                    table_field,
                    query_field
                ))
            );
            return vec![str];
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            return to_selection(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );
        }

        _ => vec![],
    }
}

// FROM
//
fn to_from(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.append(&mut to_subfrom(
            2,
            context,
            table,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subfrom(
    indent: usize,
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let table_name = ast::get_tablename(&table.name, &table.fields);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            let foreign_table_name =
                ast::get_tablename(&link.foreign_tablename, &link_table.fields);
            let mut inner_list = to_from(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );
            let join = format!(
                "left join {} on {}.{} = {}.{}",
                quote(&foreign_table_name),
                quote(&table_name),
                quote(&link.local_ids.join("")),
                quote(&foreign_table_name),
                quote(&link.foreign_ids.join("")),
            );
            inner_list.push(join);
            return inner_list;
        }

        _ => vec![],
    }
}

// Field names

fn to_fieldnames(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        let table_name = ast::get_tablename(&table.name, &table.fields);

        result.append(&mut to_table_fieldname(
            2,
            context,
            &table_name,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_table_fieldname(
    indent: usize,
    context: &typecheck::Context,
    table_name: &str,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            let str = query_field.name.to_string();
            return vec![str];
        }
        // ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
        //     let spaces = " ".repeat(indent);

        //     let foreign_table_alias = match query_field.alias {
        //         Some(ref alias) => &alias,
        //         None => &link.foreign_tablename,
        //     };
        //     let link_table = typecheck::get_linked_table(context, &link).unwrap();
        //     return to_selection(
        //         context,
        //         &ast::get_aliased_name(&query_field),
        //         link_table,
        //         &ast::collect_query_fields(&query_field.fields),
        //     );
        // }
        _ => vec![],
    }
}

// SET values

fn to_field_set_values(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        match &field.set {
            None => (),
            Some(val) => match table_field {
                ast::Field::Column(column) => {
                    let mut str = String::new();

                    str.push_str(&column.name);
                    str.push_str(" = ");
                    str.push_str(&to_sql::render_value(&val));

                    result.push(str);
                }
                _ => (),
            },
        }
    }

    result
}

// Insert
fn to_field_insert_values(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        match &field.set {
            None => (),
            Some(val) => {
                let spaces = " ".repeat(2);
                let str = to_sql::render_value(&val);
                result.push(str);
            }
        }
    }

    result
}

// WHERE
//
fn to_where(
    context: &typecheck::Context,
    table_name: &str,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.append(&mut to_subwhere(
            2,
            context,
            table_alias,
            &table_field,
            &field,
        ));
    }

    result
}

fn to_subwhere(
    indent: usize,
    context: &typecheck::Context,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            return render_where_params(&ast::collect_query_args(&query_field.fields), table_alias);
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            let foreign_table_name =
                ast::get_tablename(&link.foreign_tablename, &link_table.fields);
            let mut inner_list = to_where(
                context,
                &foreign_table_name,
                &ast::get_aliased_name(&query_field),
                link_table,
                &ast::collect_query_fields(&query_field.fields),
            );

            return inner_list;
        }

        _ => vec![],
    }
}

fn render_where_params(args: &Vec<ast::Arg>, table_alias: &str) -> Vec<String> {
    let mut result = vec![];
    for where_arg in ast::collect_where_args(args) {
        result.push(render_where_arg(&where_arg, table_alias));
    }
    result
}

fn render_where_arg(arg: &ast::WhereArg, table_alias: &str) -> String {
    match arg {
        ast::WhereArg::Column(name, operator, value) => {
            let qualified_column_name =
                format!("{}.{}", format_tablename(table_alias), quote(name));
            let operator = match operator {
                ast::Operator::Equal => "=",
                ast::Operator::NotEqual => "!=",
                ast::Operator::GreaterThan => ">",
                ast::Operator::LessThan => "<",
                ast::Operator::GreaterThanOrEqual => ">=",
                ast::Operator::LessThanOrEqual => "<=",
                ast::Operator::In => "in",
                ast::Operator::NotIn => "not in",
                ast::Operator::Like => "like",
                ast::Operator::NotLike => "not like",
            };
            let value = to_sql::render_value(value);
            format!("{} {} {}", qualified_column_name, operator, value)
        }
        ast::WhereArg::And(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table_alias));
            }
            format!("({})", inner_list.join(" and "))
        }
        ast::WhereArg::Or(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table_alias));
            }
            format!("({})", inner_list.join(" or "))
        }
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s)
}

fn format_tablename(name: &str) -> String {
    format!("\"{}\"", crate::ext::string::decapitalize(name))
}
