use crate::ast;
use crate::ext::string;
use crate::generate::sql;
use crate::typecheck;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn schema(schem: &ast::Schema) -> String {
    let mut result = String::new();

    result.push_str("\n\n");

    for definition in &schem.definitions {
        result.push_str(&to_string_definition(definition));
    }
    result
}

fn to_string_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Tagged { name, variants } => {
            let mut result = format!("type {} =", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str("\n");
                result.push_str(&to_string_variant(is_first, 2, variant));

                is_first = false;
            }
            result.push_str(";\n\n");
            result
        }
        ast::Definition::Record { name, fields } => to_type_alias(name, fields),
    }
}

fn to_type_alias(name: &str, fields: &Vec<ast::Field>) -> String {
    let mut result = format!("type {} = {{\n  ", name);

    let mut is_first = true;
    for field in fields {
        if (ast::is_column_space(field)) {
            continue;
        }

        result.push_str(&to_string_field(is_first, 2, &field));

        if is_first & ast::is_column(field) {
            is_first = false;
        }
    }
    result.push_str("};\n");
    result
}

fn to_string_variant(is_first: bool, indent_size: usize, variant: &ast::Variant) -> String {
    let prefix = " | ";

    match &variant.data {
        Some(fields) => {
            let indent = " ".repeat(indent_size + 4);

            let mut result = format!(
                " {}{{\n{}\"type\": {};\n{}",
                prefix,
                indent,
                crate::ext::string::quote(&variant.name),
                indent
            );

            let mut is_first_field = true;
            for field in fields {
                result.push_str(&to_string_field(is_first_field, indent_size + 4, &field));
                is_first_field = false
            }
            result.push_str("    }");
            result
        }
        None => format!(
            " {}{{ \"type\": {} }}",
            prefix,
            crate::ext::string::quote(&variant.name)
        ),
    }
}

fn to_string_field(is_first: bool, indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::ColumnLines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Field::Column(column) => to_string_column(is_first, indent, column),
        ast::Field::ColumnComment { text } => "".to_string(),
        ast::Field::FieldDirective(directive) => "".to_string(),
    }
}

fn to_string_column(is_first: bool, indent: usize, column: &ast::Column) -> String {
    if is_first {
        return format!(
            "{}: {};\n",
            crate::ext::string::quote(&column.name),
            to_ts_typename(false, &column.type_)
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}{}: {};\n",
            spaces,
            crate::ext::string::quote(&column.name),
            to_ts_typename(false, &column.type_)
        );
    }
}

// DECODE
//

pub fn to_schema_decoders(schem: &ast::Schema) -> String {
    let mut result = String::new();

    result.push_str("import * as Ark from 'arktype';");

    result.push_str("\n\n");

    for definition in &schem.definitions {
        result.push_str(&to_decoder_definition(definition));
    }
    result
}

fn to_decoder_definition(definition: &ast::Definition) -> String {
    match definition {
        ast::Definition::Lines { count } => {
            if (*count > 2) {
                "\n\n".to_string()
            } else {
                "\n".repeat(*count as usize)
            }
        }
        ast::Definition::Comment { text } => "".to_string(),
        ast::Definition::Tagged { name, variants } => {
            let mut result = "".to_string();

            result.push_str(&format!("export const {} = Ark.union(\n", name));
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_decoder_variant(is_first, 2, name, variant));
                is_first = false;
            }
            result.push_str(");\n");
            // result.push_str("        |> Db.Read.custom\n");
            result
        }
        ast::Definition::Record { name, fields } => "".to_string(),
    }
}

fn to_decoder_variant(
    is_first: bool,
    indent_size: usize,
    typename: &str,
    variant: &ast::Variant,
) -> String {
    let outer_indent = " ".repeat(indent_size);
    let indent = " ".repeat(indent_size + 4);
    let inner_indent = " ".repeat(indent_size + 8);
    match &variant.data {
        Some(fields) => {
            let mut result = format!(
                "{}Ark.object({{\n    \"type_\": Ark.literal({}),\n",
                outer_indent,
                crate::ext::string::quote(&variant.name),
            );

            for field in fields {
                result.push_str(&to_variant_field_json_decoder(indent_size + 2, &field));
            }
            result.push_str(&format!("{}}}),\n", outer_indent));

            result
        }
        None => format!(
            "{}Ark.object({{ \"type_\": Ark.literal({}) }}),\n",
            outer_indent,
            crate::ext::string::quote(&variant.name),
        ),
    }
}

// Field directives(specifically @link) is not allowed within a type at the moment
fn to_variant_field_json_decoder(indent: usize, field: &ast::Field) -> String {
    match field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}{}: {},\n",
                spaces,
                crate::ext::string::quote(&column.name),
                to_ts_type_decoder(true, &column.type_)
            );
        }
        _ => "".to_string(),
    }
}

fn to_json_type_decoder(type_: &str) -> String {
    match type_ {
        "String" => "Decode.string".to_string(),
        "Int" => "Decode.int".to_string(),
        "Float" => "Decode.float".to_string(),
        _ => crate::ext::string::decapitalize(type_).to_string(),
    }
}

fn to_type_decoder(type_: &str) -> String {
    match type_ {
        "String" => "Db.Read.string".to_string(),
        "Int" => "Db.Read.int".to_string(),
        "Float" => "Db.Read.float".to_string(),
        _ => format!("Db.Decode.{}", crate::ext::string::decapitalize(type_)).to_string(),
    }
}

//  QUERIES
//
pub fn write_queries(
    dir: &str,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
) -> io::Result<()> {
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let path = &format!(
                    "{}/query/{}.ts",
                    dir,
                    crate::ext::string::decapitalize(&q.name)
                );
                let target_path = Path::new(path);
                let mut output = fs::File::create(target_path).expect("Failed to create file");
                output
                    .write_all(to_query_file(&context, &q).as_bytes())
                    .expect("Failed to write to file");
            }
            _ => continue,
        }
    }
    Ok(())
}

pub fn literal_quote(s: &str) -> String {
    format!("`\n{}`", s)
}

fn to_query_file(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = "".to_string();
    result.push_str("import * as Ark from 'arktype';\n");
    result.push_str("import * as Db from '../db.ts';\n");
    result.push_str("import * as Decode from '../db/decode.ts';\n\n");

    // Input args decoder
    to_query_input_decoder(context, &query, &mut result);

    let validate = r#"

type Input = typeof Input.infer

type Failure = { error: Ark.ArkError }

type Success = { sql: string, args: any }

export const check = (input: any): Success | Failure => {
    const parsed = Input(input);
    if (parsed instanceof Ark.type.errors) {
        return { error: parsed }
    } else {
        return { sql: sql, args: parsed }
    }
})"#;

    result.push_str(validate);

    let sql = crate::generate::sql::to_string(context, query);
    result.push_str("\n\nconst sql = ");
    result.push_str(&literal_quote(&sql));
    result.push_str(";\n\n");

    // Type Alisaes
    // result.push_str("// Return data\n");
    // for field in &query.fields {
    //     let table = context.tables.get(&field.name).unwrap();
    //     result.push_str(&to_query_type_alias(
    //         context,
    //         table,
    //         &field.name,
    //         &ast::collect_query_fields(&field.fields),
    //     ));
    // }

    // TODO:: HTTP Sender

    // Nested Return data decoders
    // result.push_str("\n\n");
    // for field in &query.fields {
    //     let table = context.tables.get(&field.name).unwrap();
    //     result.push_str(&to_query_decoder(
    //         context,
    //         &ast::get_aliased_name(&field),
    //         table,
    //         &ast::collect_query_fields(&field.fields),
    //     ));
    // }
    //

    // Rectangle data decoder
    result.push_str("\n");
    result.push_str("export const ReturnRectangle = Ark.object({\n");
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();

        to_flat_query_decoder(
            context,
            &ast::get_aliased_name(&field),
            table,
            &ast::collect_query_fields(&field.fields),
            &mut result,
        );
    }
    result.push_str("});");

    result
}

fn to_query_input_decoder(context: &typecheck::Context, query: &ast::Query, result: &mut String) {
    result.push_str("export const Input = Ark.object({{");
    for arg in &query.args {
        result.push_str(&format!(
            "\n  {}: {},",
            crate::ext::string::quote(&arg.name),
            to_ts_type_decoder(true, &arg.type_)
        ));
    }
    result.push_str("\n});\n");
}

fn to_flat_query_decoder(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
    result: &mut String,
) {
    // let mut result = format!(
    //     "decode{} : Db.Read.Query {}\n",
    //     crate::ext::string::capitalize(table_alias),
    //     crate::ext::string::capitalize(table_alias)
    // );

    let identifiers = format!("[]");

    // result.push_str(&format!(
    //     "decode{} =\n",
    //     crate::ext::string::capitalize(table_alias)
    // ));
    // result.push_str(&format!(
    //     "    Db.Read.query {} {}\n",
    //     crate::ext::string::capitalize(table_alias),
    //     identifiers
    // ));
    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        to_table_field_flat_decoder(2, context, table_alias, table_field, field, result)
    }
}

fn to_table_field_flat_decoder(
    indent: usize,
    context: &typecheck::Context,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
    result: &mut String,
) {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            result.push_str(&format!(
                "{}\"{}\": {},\n",
                spaces,
                ast::get_select_alias(table_alias, table_field, query_field),
                to_ts_type_decoder(true, &column.type_)
            ));
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };

            let table = typecheck::get_linked_table(context, &link).unwrap();

            to_flat_query_decoder(
                context,
                foreign_table_alias,
                table,
                &ast::collect_query_fields(&query_field.fields),
                result,
            )

            // result.push_str(&format!(
            //     "{}|> Db.Read.nested\n{}({})\n{}({})\n{}decode{}\n",
            //     spaces,
            //     // ID columns
            //     " ".repeat(indent + 4),
            //     format_db_id(table_alias, &link.local_ids),
            //     " ".repeat(indent + 4),
            //     format_db_id(foreign_table_alias, &link.foreign_ids),
            //     " ".repeat(indent + 4),
            //     (crate::ext::string::capitalize(&ast::get_aliased_name(query_field))) // (capitalize(&link.link_name)) // ast::get_select_alias(table_alias, table_field, query_field),
            // ));
        }

        _ => (),
    }
}

fn to_query_decoder(
    context: &typecheck::Context,
    table_alias: &str,
    table: &ast::RecordDetails,
    fields: &Vec<&ast::QueryField>,
) -> String {
    let mut result = format!(
        "decode{} : Db.Read.Query {}\n",
        crate::ext::string::capitalize(table_alias),
        crate::ext::string::capitalize(table_alias)
    );

    let identifiers = format!("[]");

    result.push_str(&format!(
        "decode{} =\n",
        crate::ext::string::capitalize(table_alias)
    ));
    result.push_str(&format!(
        "    Db.Read.query {} {}\n",
        crate::ext::string::capitalize(table_alias),
        identifiers
    ));
    for field in fields {
        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        result.push_str(&to_table_field_decoder(
            8,
            table_alias,
            &table_field,
            &field,
        ));
    }

    for field in fields {
        if field.fields.is_empty() {
            continue;
        }

        let fieldname_match = table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(f, &field.name));

        match fieldname_match {
            Some(ast::Field::FieldDirective(ast::FieldDirective::Link(link))) => {
                let link_table = typecheck::get_linked_table(context, &link).unwrap();
                result.push_str("\n\n");
                result.push_str(&to_query_decoder(
                    context,
                    &ast::get_aliased_name(&field),
                    link_table,
                    &ast::collect_query_fields(&field.fields),
                ));
            }
            _ => continue,
        }
    }

    result
}

fn format_db_id(table_alias: &str, ids: &Vec<String>) -> String {
    let mut result = String::new();
    for id in ids {
        let formatted = format!("{}__{}", table_alias, id);
        result.push_str(&format!("Db.Read.id \"{}\"", formatted));
    }
    result
}

fn to_table_field_decoder(
    indent: usize,
    table_alias: &str,
    table_field: &ast::Field,
    query_field: &ast::QueryField,
) -> String {
    match table_field {
        ast::Field::Column(column) => {
            let spaces = " ".repeat(indent);
            return format!(
                "{}|> Db.Read.field \"{}\" {}\n",
                spaces,
                ast::get_select_alias(table_alias, table_field, query_field),
                to_type_decoder(&column.type_)
            );
        }
        ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
            let spaces = " ".repeat(indent);

            let foreign_table_alias = match query_field.alias {
                Some(ref alias) => &alias,
                None => &link.foreign_tablename,
            };

            return format!(
                "{}|> Db.Read.nested\n{}({})\n{}({})\n{}decode{}\n",
                spaces,
                // ID columns
                " ".repeat(indent + 4),
                format_db_id(table_alias, &link.local_ids),
                " ".repeat(indent + 4),
                format_db_id(foreign_table_alias, &link.foreign_ids),
                " ".repeat(indent + 4),
                (crate::ext::string::capitalize(&ast::get_aliased_name(query_field))) // (capitalize(&link.link_name)) // ast::get_select_alias(table_alias, table_field, query_field),
            );
        }

        _ => "".to_string(),
    }
}

fn to_query_type_alias(
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    name: &str,
    fields: &Vec<&ast::QueryField>,
) -> String {
    let mut result = format!("type {} =\n", crate::ext::string::capitalize(name));

    let mut is_first = true;

    for field in fields {
        if is_first {
            result.push_str(&format!("    {{ ",))
        }

        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        match table_field {
            ast::Field::Column(col) => {
                result.push_str(&to_string_query_field(is_first, 4, &field, col));
            }
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                result.push_str(&to_string_query_field_link(is_first, 4, &field, link));
            }
            _ => {}
        }

        if is_first {
            is_first = false;
        }
    }
    result.push_str("    }\n");

    for field in fields {
        if field.fields.is_empty() {
            continue;
        }

        let fieldname_match = table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(f, &field.name));

        match fieldname_match {
            Some(ast::Field::FieldDirective(ast::FieldDirective::Link(link))) => {
                let link_table = typecheck::get_linked_table(context, &link).unwrap();

                result.push_str("\n\n");
                result.push_str(&to_query_type_alias(
                    context,
                    link_table,
                    &ast::get_aliased_name(field),
                    &ast::collect_query_fields(&field.fields),
                ));
            }
            _ => continue,
        }
    }

    result
}

fn to_string_query_field_link(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    link_details: &ast::LinkDetails,
) -> String {
    let field_name = ast::get_aliased_name(field);

    if is_first {
        return format!(
            "{} : {}\n",
            crate::ext::string::decapitalize(&field_name),
            (format!("List {}", crate::ext::string::capitalize(&field_name)))
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}\n",
            spaces,
            crate::ext::string::decapitalize(&field_name),
            (format!("List {}", crate::ext::string::capitalize(&field_name)))
        );
    }
}

fn to_string_query_field(
    is_first: bool,
    indent: usize,
    field: &ast::QueryField,
    table_column: &ast::Column,
) -> String {
    let field_name = ast::get_aliased_name(field);
    if is_first {
        return format!(
            "{} : {}\n",
            crate::ext::string::decapitalize(&field_name),
            to_ts_typename(true, &table_column.type_)
        );
    } else {
        let spaces = " ".repeat(indent);
        return format!(
            "{}, {} : {}\n",
            spaces,
            crate::ext::string::decapitalize(&field_name),
            to_ts_typename(true, &table_column.type_)
        );
    }
}

fn to_ts_typename(qualified: bool, type_: &str) -> String {
    match type_ {
        "String" => "string".to_string(),
        "Int" => "number".to_string(),
        "Float" => "number".to_string(),
        "Bool" => "boolean".to_string(),
        _ => {
            let qualification = if qualified { "Db." } else { "" };
            return format!("{}{}", qualification, type_).to_string();
        }
    }
}

fn to_ts_type_decoder(qualified: bool, type_: &str) -> String {
    match type_ {
        "String" => "Ark.string".to_string(),
        "Int" => "Ark.number".to_string(),
        "Float" => "Ark.number".to_string(),
        "Bool" => "Ark.boolean".to_string(),
        _ => {
            let qualification = if qualified { "Decode." } else { "" };
            return format!("{}{}", qualification, type_).to_string();
        }
    }
}

// Example: ($arg: String)
fn to_string_param_definition(is_first: bool, param: &ast::QueryParamDefinition) -> String {
    if (is_first) {
        format!("{}: {}", param.name, param.type_)
    } else {
        format!(", {}: {}", param.name, param.type_)
    }
}

fn value_to_string(value: &ast::QueryValue) -> String {
    match value {
        ast::QueryValue::Variable(name) => format!("${}", name),
        ast::QueryValue::String(value) => format!("\"{}\"", value),
        ast::QueryValue::Int(value) => value.to_string(),
        ast::QueryValue::Float(value) => value.to_string(),
        ast::QueryValue::Bool(value) => value.to_string(),
        ast::QueryValue::Null => "null".to_string(),
    }
}

fn operator_to_string(operator: &ast::Operator) -> &str {
    match operator {
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
    }
}
