use crate::ast;
use crate::ext::string;
use crate::generate::sql::select;
use crate::generate::sql::to_sql;
use crate::typecheck;

/*

See the temp_tables/mod.rs to see an overview of the sql strategy we want here.


The general algorithm.

1. Insert a value into the current table.
2. If there is a nested insert, create a temporary table with the name format of _temp_inserted_{table_field_alias}
    3. recursively generate for next nested insert.
4. Delete temp table.




*/

pub fn insert_to_string(
    context: &typecheck::Context,
    query: &ast::Query,
    query_info: &typecheck::QueryInfo,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
) -> Vec<to_sql::Prepared> {
    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);

    let mut statements = to_sql::format_attach(query_info);
    statements.push(to_sql::ignore(initial_select(
        0,
        context,
        query,
        table,
        query_table_field,
    )));

    let parent_temp_table_name = &get_temp_table_name(&query_table_field);
    let mut temp_table_created = false;
    let mut multiple_table_inserts = false;

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
                multiple_table_inserts = true;

                if !temp_table_created {
                    statements.push(to_sql::ignore(format!(
                        "create temp table {} as\n  select last_insert_rowid() as id",
                        parent_temp_table_name
                    )));

                    temp_table_created = true;
                }

                insert_linked(
                    0,
                    context,
                    query,
                    parent_temp_table_name,
                    linked_table,
                    query_field,
                    link,
                    &mut statements,
                );
            }
            _ => (),
        }
    }

    // The final selection
    if multiple_table_inserts {
        let mut final_statement = String::new();
        // Select the final result
        final_statement.push_str("select\n");
        let selected = &select::to_selection(
            context,
            &ast::get_aliased_name(&query_table_field),
            table,
            query_info,
            &ast::collect_query_fields(&query_table_field.fields),
            &select::TableAliasKind::Normal,
        );
        final_statement.push_str("  ");
        final_statement.push_str(&selected.join(",\n  "));
        final_statement.push_str("\n");
        select::render_from(
            context,
            table,
            query_info,
            query_table_field,
            &select::TableAliasKind::Normal,
            &mut final_statement,
        );

        let primary_table_name = select::get_tablename(
            &select::TableAliasKind::Normal,
            table,
            &ast::get_aliased_name(&query_table_field),
        );

        final_statement.push_str(&format!(
            "where\n  {}.rowid in (select id from {})",
            primary_table_name, parent_temp_table_name
        ));

        statements.push(to_sql::include(final_statement));
    }

    if multiple_table_inserts {
        drop_temp_tables(query_table_field, &mut statements);
    }

    statements
}

fn drop_temp_tables(query_field: &ast::QueryField, statements: &mut Vec<to_sql::Prepared>) {
    statements.push(to_sql::ignore(drop_table(query_field)));

    // Only the primary field has a temp table created for it now
    // for arg_field in query_field.fields.iter() {
    //     match arg_field {
    //         ast::ArgField::Field(field) => {
    //             if !field.fields.is_empty() {
    //                 drop_temp_tables(field, statements);
    //             }
    //         }
    //         _ => continue,
    //     }
    // }
}

fn drop_table(query_field: &ast::QueryField) -> String {
    format!("drop table {}", &get_temp_table_name(&query_field))
}

pub fn get_temp_table_name(query_field: &ast::QueryField) -> String {
    format!("inserted_{}", &ast::get_aliased_name(&query_field))
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
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.append(&mut new_fieldnames.clone());

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);
    
    // Check if updatedAt field exists in table and is not explicitly set
    let has_updated_at_field = table.record.fields.iter().any(|f| ast::has_fieldname(f, "updatedAt"));
    let updated_at_explicitly_set = all_query_fields.iter().any(|f| f.name == "updatedAt");
    
    if has_updated_at_field && !updated_at_explicitly_set {
        field_names.push("updatedAt".to_string());
    }

    let mut result = format!(
        "{}insert into {} ({})\n",
        indent_str,
        table_name,
        field_names.join(", ")
    );

    let values = &to_field_insert_values(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &all_query_fields,
    );
    
    let mut final_values = values.clone();
    if has_updated_at_field && !updated_at_explicitly_set {
        final_values.push("unixepoch()".to_string());
    }

    result.push_str(&format!("{}values ({})", indent_str, final_values.join(", ")));
    result
}

pub fn insert_linked(
    indent: usize,
    context: &typecheck::Context,
    query: &ast::Query,
    parent_table_name: &String,
    table: &typecheck::Table,
    query_table_field: &ast::QueryField,
    link: &ast::LinkDetails,
    statements: &mut Vec<to_sql::Prepared>,
) {
    // INSERT INTO users (username, credit) VALUES ('john_doe', 100);
    let mut field_names: Vec<String> = Vec::new();

    let table_name = ast::get_tablename(&table.record.name, &table.record.fields);
    let new_fieldnames = &to_fieldnames(
        context,
        &ast::get_aliased_name(&query_table_field),
        table,
        &ast::collect_query_fields(&query_table_field.fields),
    );
    field_names.push(link.foreign.fields.clone().join(", "));
    field_names.append(&mut new_fieldnames.clone());

    let all_query_fields = ast::collect_query_fields(&query_table_field.fields);
    
    // Check if updatedAt field exists in table and is not explicitly set
    let has_updated_at_field = table.record.fields.iter().any(|f| ast::has_fieldname(f, "updatedAt"));
    let updated_at_explicitly_set = all_query_fields.iter().any(|f| f.name == "updatedAt");
    
    if has_updated_at_field && !updated_at_explicitly_set {
        field_names.push("updatedAt".to_string());
    }

    let mut insert_values = vec![];
    for local_id in &link.local_ids {
        insert_values.push(format!(
            "{}.{}",
            string::quote(parent_table_name),
            string::quote(&local_id)
        ));
    }

    for query_field in &all_query_fields {
        match &query_field.set {
            None => (),
            Some(val) => {
                let str = to_sql::render_value(&val);
                insert_values.push(str);
            }
        }
    }
    
    if has_updated_at_field && !updated_at_explicitly_set {
        insert_values.push("unixepoch()".to_string());
    }

    statements.push(to_sql::ignore(format!(
        "insert into {} ({})\n  select {}\n  from {}",
        table_name,
        field_names.join(", "),
        insert_values.join(", "),
        parent_table_name
    )));

    let temp_table_name = &get_temp_table_name(&query_table_field);

    // We could save the inserted id here, but I don't think we need to?
    // statements.push(to_sql::ignore(format!(
    //     "create temp table {} as\n  select last_insert_rowid() as id",
    //     temp_table_name
    // )));

    for query_field in all_query_fields {
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

                insert_linked(
                    indent + 2,
                    context,
                    query,
                    &temp_table_name,
                    linked_table,
                    query_field,
                    &link,
                    statements,
                );
            }
            _ => (),
        }
    }
}

// Field names

fn to_fieldnames(
    context: &typecheck::Context,
    table_alias: &str,
    table: &typecheck::Table,
    query_fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in query_fields {
        let table_field = &table
            .record
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        let table_name = ast::get_tablename(&table.record.name, &table.record.fields);

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
        ast::Field::Column(_) => {
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
    table: &typecheck::Table,
    fields: &Vec<&ast::QueryField>,
) -> Vec<String> {
    let mut result = vec![];

    for field in fields {
        match &field.set {
            None => (),
            Some(val) => {
                let str = to_sql::render_value(&val);
                result.push(str);
            }
        }
    }

    result
}
