use crate::ast;
use crate::ext::string;
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
) -> Vec<to_sql::Prepared> {
    let mut statements = to_sql::format_attach(query_info);

    let mut selection = String::new();
    selection.push_str("select\n");

    // Selection

    let selected = &to_selection(
        context,
        &ast::get_aliased_name(&query_field),
        table,
        query_info,
        &ast::collect_query_fields(&query_field.fields),
        &TableAliasKind::Normal,
    );
    selection.push_str("  ");
    selection.push_str(&selected.join(",\n  "));
    selection.push_str("\n");

    // FROM
    render_from(
        context,
        table,
        query_info,
        query_field,
        &TableAliasKind::Normal,
        &mut selection,
    );

    // WHERE
    to_sql::render_where(
        context,
        table,
        query_info,
        query_field,
        &ast::QueryOperation::Select,
        &mut selection,
    );

    // Order by
    to_sql::render_order_by(query_field, &mut selection);

    // LIMIT
    to_sql::render_limit(query_field, &mut selection);

    // OFFSET
    to_sql::render_offset(query_field, &mut selection);

    statements.push(to_sql::include(selection));

    statements
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
    context: &typecheck::Context,
    table: &typecheck::Table,
    table_alias: &str,
    table_field: &ast::Field,
    query_info: &typecheck::QueryInfo,
    query_field: &ast::QueryField,
    table_alias_kind: &TableAliasKind,
) -> Vec<String> {
    match table_field {
        ast::Field::Column(table_column) => {
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
            match &table_column.serialization_type {
                ast::SerializationType::Concrete(_) => {
                    // A single concrete type
                    let str = format!(
                        "{} as {}",
                        source_field,
                        string::quote(&ast::get_select_alias(table_alias, query_field))
                    );
                    return vec![str];
                }
                ast::SerializationType::FromType(typename) => {
                    // We don't know what this type is
                    let mut selected = vec![];
                    select_type_columns(context, typename, &mut selected);
                    return selected;
                }
            }
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
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

fn select_type_columns(context: &typecheck::Context, typename: &str, selection: &mut Vec<String>) {
    match context.types.get(typename) {
        None => return,
        Some((definfo, type_)) => {
            // TODOOOOO
            //  LEAVING THIS TO START ON THE JSON VERSION
            // Trying to recreate this form:
            // CASE
            //     WHEN status = 'Active' THEN
            //         json_object(
            //             '$', 'Active',
            //             'activatedAt', status_active_activatedAt
            //         )
            //     ELSE
            //         json_object(
            //             '$', 'Inactive'
            //         )
            // END

            match type_ {
                typecheck::Type::OneOf { variants } => for var in variants {},

                _ => return,
            }
            // let str = format!(
            //     "{} as {}",
            //     source_field,
            //     string::quote(&ast::get_select_alias(
            //         table_alias,
            //         table_field,
            //         query_field
            //     ))
            // );
            // return vec![str];
        }
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
    if from_vals.is_empty() {
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

pub fn get_tablename(
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

            let link_table = typecheck::get_linked_table(context, &link).unwrap();

            let foreign_table_name = get_tablename(
                table_alias_kind,
                link_table,
                &ast::get_aliased_name(&query_field),
            );

            let mut inner_list = to_from(
                context,
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
