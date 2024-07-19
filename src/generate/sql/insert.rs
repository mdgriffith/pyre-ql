use crate::ast;
use crate::ext::string;
use crate::generate::sql::to_sql;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

/*
In a simple case, we can do a normal insert, but if we want nested inserts
we need to do something like this:


    WITH inserted_rulebook AS (
        INSERT INTO rulebooks (name, publisherId, createdAt, updatedAt)
        VALUES ('New Rulebook', NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        RETURNING id
    ),
    inserted_rulebooks_owned AS (
        INSERT INTO rulebooks_owned (userId, rulebookId, createdAt)
        SELECT $1, id, CURRENT_TIMESTAMP
        FROM inserted_rulebook
        RETURNING id
    )
    INSERT INTO rulebook_details (rulebookOwnedId, detail, createdAt)
    SELECT id, 'Detail for new rulebook', CURRENT_TIMESTAMP
    FROM inserted_rulebooks_owned;



*/

pub fn insert_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    table: &ast::RecordDetails,
    query_table_field: &ast::QueryField,
) -> String {
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut last_link_index = 0;
    for (i, query_field) in all_query_fields.iter().enumerate() {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                last_link_index = i;
            }
            _ => (),
        }
    }
    let initial_indent = if (last_link_index == 0) { 0 } else { 4 };
    let mut result = String::new();
    let mut initial_selection =
        initial_select(initial_indent, context, query, table, query_table_field);
    let parent_table_alias = &get_temp_table_name(&query_table_field);
    let mut initial_returning: Vec<String> = vec![];
    match ast::get_primary_id_field_name(&table.fields) {
        Some(id) => initial_returning.push(id),
        None => (),
    }
    if (initial_returning.len() > 0) {
        initial_selection.push_str(&format!(
            "\n{}returning {}",
            " ".repeat(initial_indent),
            initial_returning.clone().join(", ")
        ));
    }
    let mut rendered_initial = false;

    for (i, query_field) in all_query_fields.iter().enumerate() {
        let table_field = &table
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
                    result.push_str("\n),");
                    rendered_initial = true;
                }
                let is_last = i == last_link_index;

                let inner_selection = &insert_linked(
                    if (is_last) { 0 } else { 4 },
                    context,
                    query,
                    parent_table_alias,
                    linked_table,
                    query_field,
                    link,
                );

                let temp_table_alias = &get_temp_table_name(&query_field);

                if !is_last {
                    result.push_str(" ");
                    result.push_str(temp_table_alias);
                    result.push_str(" as (");
                    result.push_str(inner_selection);
                    result.push_str("\n),");
                } else {
                    result.push_str(inner_selection);
                }
            }
            _ => (),
        }
    }

    if !rendered_initial {
        result.push_str(&initial_selection);
    }
    result.push_str(";");

    result
}

pub fn get_temp_table_name(query_field: &ast::QueryField) -> String {
    format!("inserted_{}", &ast::get_aliased_name(&query_field))
}

pub fn initial_select(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    table: &ast::RecordDetails,
    query_table_field: &ast::QueryField,
) -> String {
    let indent_str = " ".repeat(indent);
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);
    let mut field_names: Vec<String> = Vec::new();
    let mut table_name = query.name.clone();

    let table_name = ast::get_tablename(&table.name, &table.fields);
    let mut new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.append(&mut new_fieldnames.clone());

    let mut result = format!(
        "{}insert into {} ({})\n",
        indent_str,
        table_name,
        field_names.join(", ")
    );

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let values = &to_field_insert_values(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &all_query_fields,
    );

    result.push_str(&format!("{}values ({})", indent_str, values.join(", ")));
    result
}

pub fn insert_linked(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &ast::RecordDetails,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,
) -> String {
    let indent_str = " ".repeat(indent);
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);
    let mut field_names: Vec<String> = Vec::new();
    let mut table_name = query.name.clone();

    let table_name = ast::get_tablename(&table.name, &table.fields);
    let mut new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.push(link.foreign_ids.clone().join(", "));
    field_names.append(&mut new_fieldnames.clone());

    let mut result = format!(
        "\n{}insert into {} ({})\n",
        indent_str,
        table_name,
        field_names.join(", ")
    );

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut insert_values = vec![];
    for local_id in &link.local_ids {
        insert_values.push(format!(
            "{}.{}",
            string::quote(parent_table_name),
            string::quote(&local_id)
        ));
    }

    let mut returning: Vec<String> = vec![];
    match ast::get_primary_id_field_name(&table.fields) {
        Some(id) => returning.push(id),
        None => (),
    }

    for query_field in &all_query_fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match &query_field.set {
            None => (),
            Some(val) => {
                let spaces = " ".repeat(2);
                let str = to_sql::render_value(&val);
                insert_values.push(str);
            }
        }
    }

    result.push_str(&format!(
        "{}select ({})\n",
        indent_str,
        insert_values.join(", ")
    ));
    result.push_str(&format!("{}from {}", indent_str, parent_table_name));
    if (returning.len() > 0) {
        result.push_str(&format!(
            "\n{}returning {}",
            indent_str,
            returning.clone().join(", ")
        ));
    }

    for query_field in all_query_fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &query_field.name))
            .unwrap();

        match table_field {
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                // We are inserting a link, so we need to do a nested insert
                let linked_table = typecheck::get_linked_table(context, &link).unwrap();

                result.push_str(&insert_linked(
                    indent + 2,
                    context,
                    query,
                    &get_temp_table_name(&query_table_field),
                    linked_table,
                    query_field,
                    link,
                ));
            }
            _ => (),
        }
    }

    result
}

// Field names

fn to_fieldnames(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    query_fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in query_fields {
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
        _ => vec![],
    }
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