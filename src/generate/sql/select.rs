use crate::ast;
use crate::ext::string;
use crate::generate::sql::cte;
use crate::generate::sql::to_sql;
use crate::typecheck;

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
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
) -> String {
    if cte::should_use_cte(query) {
        let mut result = to_sql::format_attach(query_info);

        cte::select_to_string(context, query, query_field, &mut result);

        return result;
    }
    let mut result = to_sql::format_attach(query_info);
    result.push_str("select\n");

    // Selection

    let selected = &to_selection(
        context,
        &ast::get_aliased_name(&query_field),
        table,
        query_info,
        &ast::collect_query_fields(&query_field.fields),
        &TableAliasKind::Normal,
    );
    result.push_str("  ");
    result.push_str(&selected.join(",\n  "));
    result.push_str("\n");

    // FROM
    render_from(
        context,
        table,
        query_info,
        query_field,
        &TableAliasKind::Normal,
        &mut result,
    );

    // WHERE
    render_where(context, table, query_info, query_field, &mut result);

    // Order by
    render_order_by(context, table, query_field, &mut result);

    // LIMIT
    render_limit(context, table, query_field, &mut result);

    // OFFSET
    render_offset(context, table, query_field, &mut result);

    result.push_str(";");

    result
}

pub enum TableAliasKind {
    Normal,
    Insert,
}

// SELECT
pub fn to_selection(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    fields: &Vec<&ast::QueryField>,
    table_alias_kind: &TableAliasKind,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.append(&mut to_subselection(
            2,
            context,
            table,
            table_alias,
            &table_field,
            query_info,
            &field,
            table_alias_kind,
        ));
    }

    result
}

fn to_subselection(
    indent: usize,
    context: &typecheck::Context,
    table: &typecheck::Table,
    table_alias: &str,
    table_field: &ast::Field,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
    table_alias_kind: &TableAliasKind,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            let source_field = match table_alias_kind {
                TableAliasKind::Normal => to_sql::render_real_field(table, query_info, query_field),
                TableAliasKind::Insert => {
                    let table_name = get_tablename(table_alias_kind, table, table_alias);
                    format!(
                        "{}.{}",
                        string::quote(&table_name),
                        string::quote(&query_field.name),
                    )
                }
            };

            let str = format!(
                "{} as {}",
                source_field,
                string::quote(&ast::get_select_alias(
                    table_alias,
                    table_field,
                    query_field
                ))
            );
            return vec![str];
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign.table,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            return to_selection(
                context,
                &ast::get_aliased_name(&query_field),
                link_table,
                query_info,
                &ast::collect_query_fields(&query_field.fields),
                table_alias_kind,
            );
        }

        _ => vec![],
    }
}

// FROM
//
pub fn render_from(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_table_field: &ast::QueryField,
    table_alias_kind: &TableAliasKind,
    result: &mut String,
) {
    result.push_str("from\n");

    let table_name = get_tablename(
        table_alias_kind,
        table,
        &ast::get_aliased_name(&query_table_field),
    );

    let from_vals = &mut to_from(
        context,
        // &get_temp_table_alias(table_alias_kind, &query_table_field),
        &table_name,
        table_alias_kind,
        table,
        query_info,
        &ast::collect_query_fields(&query_table_field.fields),
    );

    // the from statements are naturally in reverse order
    // Because we're walking outwards from the root, and `.push` ing the join statements
    // Now re reverse them so they're in the correct order.
    from_vals.reverse();

    result.push_str(&format!("  {}", string::quote(&table_name)));
    if (from_vals.is_empty()) {
        result.push_str("\n");
    } else {
        result.push_str("\n  ");
        result.push_str(&from_vals.join("\n  "));
        result.push_str("\n");
    }
}

fn to_from(
    context: &typecheck::Context,
    table_alias: &str,
    table_alias_kind: &TableAliasKind,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for query_field in fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        result.append(&mut to_subfrom(
            2,
            context,
            table,
            table_alias,
            table_alias_kind,
            table_field,
            query_info,
            query_field,
        ));
    }

    result
}

fn get_tablename(
    table_alias_kind: &TableAliasKind,
    table: &typecheck::Table,
    table_alias: &str,
) -> String {
    match table_alias_kind {
        TableAliasKind::Normal => ast::get_tablename(&table.record.name, &table.record.fields),
        TableAliasKind::Insert => {
            // If this is an insert, we are selecting from a temp table
            // format!("inserted_{}", &ast::get_aliased_name(&query_field))
            format!("inserted_{}", table_alias)
        }
    }
}

fn to_subfrom(
    indent: usize,
    context: &typecheck::Context,
    table: &typecheck::Table,
    table_alias: &str,
    table_alias_kind: &TableAliasKind,
    table_field: &ast::Field,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let table_name = get_tablename(
                table_alias_kind,
                table,
                &ast::get_aliased_name(&query_field),
            );

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign.table,
            };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            // let foreign_table_name = format!(
            //     "inserted_{}",
            //     &ast::get_tablename(&link_table.record.name, &link_table.record.fields)
            // );
            let foreign_table_name = get_tablename(
                table_alias_kind,
                link_table,
                &ast::get_aliased_name(&query_field),
            );

            // get_tablename(table_alias_kind, link_table);

            let mut inner_list = to_from(
                context,
                // &ast::get_aliased_name(&query_field),
                &table_name,
                table_alias_kind,
                link_table,
                query_info,
                &ast::collect_query_fields(&query_field.fields),
            );

            let local_table_identifier = match table_alias_kind {
                TableAliasKind::Normal => {
                    to_sql::render_real_where_field(table, query_info, &link.local_ids.join(" "))
                }
                TableAliasKind::Insert => {
                    format!(
                        "{}.{}",
                        string::quote(&table_alias),
                        string::quote(&link.local_ids.join(" "))
                    )
                }
            };

            let foreign_table_identifier = match table_alias_kind {
                TableAliasKind::Normal => to_sql::render_real_where_field(
                    link_table,
                    query_info,
                    &link.foreign.fields.join(""),
                ),
                TableAliasKind::Insert => {
                    format!(
                        "{}.{}",
                        string::quote(&foreign_table_name),
                        string::quote(&link.foreign.fields.join(""))
                    )
                }
            };

            let join = format!(
                "left join {} on {} = {}",
                string::quote(&foreign_table_name),
                local_table_identifier,
                foreign_table_identifier
            );
            inner_list.push(join);
            return inner_list;
        }

        _ => vec![],
    }
}

fn render_order_by(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    result: &mut String,
) {
    let mut order_vals = vec![];

    let table_alias = &ast::get_aliased_name(&query_field);

    for field in &query_field.fields {
        match field {
            ast::ArgField::Arg(located_arg) => {
                if let ast::Arg::OrderBy(dir, col) = &located_arg.arg {
                    let order_direction = ast::direction_to_string(dir);
                    order_vals.push(format!(
                        "{}.{} {}",
                        string::quote(table_alias),
                        string::quote(col),
                        order_direction
                    ));
                }
            }
            _ => continue,
        }
    }

    if !&order_vals.is_empty() {
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
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    result: &mut String,
) {
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

fn render_offset(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_field: &ast::QueryField,
    result: &mut String,
) {
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

pub fn render_where(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
    result: &mut String,
) {
    let mut where_vals = vec![];

    let new_params = render_where_params(
        &ast::collect_query_args(&query_field.fields),
        table,
        query_info,
        query_field,
    );

    where_vals.extend(new_params);

    let new_where_vals = to_where(
        context,
        table,
        query_info,
        &ast::collect_query_fields(&query_field.fields),
    );

    where_vals.extend(new_where_vals);

    if !&where_vals.is_empty() {
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

// WHERE
//
fn to_where(
    context: &typecheck::Context,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    for query_field in query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        result.append(&mut to_subwhere(
            2,
            context,
            table,
            table_field,
            query_info,
            query_field,
        ));
    }

    result
}

fn to_subwhere(
    indent: usize,
    context: &typecheck::Context,
    table: &typecheck::Table,
    table_field: &ast::Field,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(column) => {
            return render_where_params(
                &ast::collect_query_args(&query_field.fields),
                table,
                query_info,
                query_field,
            );
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            // let foreign_table_alias = match query_field.alias {
            //     Some(ref alias) => &alias,
            //     None => &link.foreign.table,
            // };
            let link_table = typecheck::get_linked_table(context, &link).unwrap();
            // let foreign_table_name =
            //     ast::get_tablename(&link.foreign.table, &link_table.record.fields);
            let inner_list = to_where(
                context,
                // &ast::get_aliased_name(&query_field),
                link_table,
                query_info,
                &ast::collect_query_fields(&query_field.fields),
            );

            return inner_list;
        }

        _ => vec![],
    }
}

fn render_where_params(
    args: &Vec<ast::Arg>,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> Vec<String> {
    let mut result = vec![];
    for where_arg in ast::collect_where_args(args) {
        result.push(render_where_arg(&where_arg, table, query_info, query_field));
    }
    result
}

fn render_where_arg(
    arg: &ast::WhereArg,
    table: &typecheck::Table,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
) -> String {
    match arg {
        ast::WhereArg::Column(fieldname, operator, value) => {
            let qualified_column_name =
                to_sql::render_real_where_field(table, query_info, fieldname);

            let operator = to_sql::operator(operator);

            let value = to_sql::render_value(value);
            format!("{} {} {}", qualified_column_name, operator, value)
        }
        ast::WhereArg::And(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table, query_info, query_field));
            }
            format!("({})", inner_list.join(" and "))
        }
        ast::WhereArg::Or(args) => {
            let mut inner_list = vec![];
            for arg in args {
                inner_list.push(render_where_arg(arg, table, query_info, query_field));
            }
            format!("({})", inner_list.join(" or "))
        }
    }
}
