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
    for file in &schem.files {
        for definition in &file.definitions {
            result.push_str(&to_string_definition(definition));
        }
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
        ast::Definition::Tagged {
            name,
            variants,
            start,
            end,
        } => {
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
        ast::Definition::Record {
            name,
            fields,
            start,
            end,
            start_name,
            end_name,
        } => to_type_alias(name, fields),
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

    for file in &schem.files {
        for definition in &file.definitions {
            result.push_str(&to_decoder_definition(definition));
        }
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
        ast::Definition::Tagged {
            name,
            variants,
            start,
            end,
        } => {
            let mut result = "".to_string();

            result.push_str(&format!("export const {} = ", name));
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_decoder_variant(is_first, 2, name, variant));
                is_first = false;
            }
            result.push_str("\n");
            // result.push_str("        |> Db.Read.custom\n");
            result
        }
        ast::Definition::Record {
            name,
            fields,
            start,
            end,
            start_name,
            end_name,
        } => "".to_string(),
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

    let or = &format!("{}{}", outer_indent, ".or");
    let starter = if is_first { "Ark.type" } else { or };

    match &variant.data {
        Some(fields) => {
            let mut result = format!(
                "{}({{\n    \"type_\": {},\n",
                starter,
                crate::ext::string::quote(&crate::ext::string::single_quote(&variant.name)),
            );

            for field in fields {
                result.push_str(&to_variant_field_json_decoder(indent_size + 2, &field));
            }
            result.push_str(&format!("{}}})\n", outer_indent));

            result
        }
        None => format!(
            "{}({{ \"type_\": {} }})\n",
            starter,
            crate::ext::string::quote(&crate::ext::string::single_quote(&variant.name)),
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
                to_ts_type_decoder(true, column.nullable, &column.type_)
            );
        }
        _ => "".to_string(),
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
    dir: &Path,
    context: &typecheck::Context,
    query_list: &ast::QueryList,
) -> io::Result<()> {
    write_runner(dir, context, query_list);
    write_watched(dir, context);

    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                let target_path = dir.join(&format!(
                    "query/{}.ts",
                    crate::ext::string::decapitalize(&q.name)
                ));

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

fn operation_name(operation: ast::QueryOperation) -> String {
    match operation {
        ast::QueryOperation::Select => "Queried",
        ast::QueryOperation::Insert => "Added",
        ast::QueryOperation::Update => "Updated",
        ast::QueryOperation::Delete => "Deleted",
    }
    .to_string()
}
fn write_watched(dir: &Path, context: &typecheck::Context) {
    let target_path = dir.join("watched.ts");

    let mut content = String::new();

    content
        .push_str("\n\n// All tables that are currently being watched\nexport enum WatchedKind {");
    let mut at_least_one_watched = false;
    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            content.push_str(&format!(
                "\n  {}{},",
                record.name,
                operation_name(watched_operation)
            ));
        }
    }
    content.push_str("\n}");

    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            let name = format!("{}{}", record.name, operation_name(watched_operation));
            content.push_str(&format!(
                "\n\nexport interface {} {{\n  kind: WatchedKind.{};\n  data: {};\n}}",
                name, name, "{}"
            ));
        }
    }

    content.push_str("\n\nexport type Watched");
    let mut at_least_one_constructor = false;
    for (name, record) in &context.tables {
        for watched_operation in ast::to_watched_operations(record) {
            let name = format!("{}{}", record.name, operation_name(watched_operation));
            if !at_least_one_constructor {
                content.push_str(&format!("\n    = {}", name));
                at_least_one_constructor = true;
            } else {
                content.push_str(&format!("\n    | {}", name));
            }
        }
    }
    if !at_least_one_constructor {
        content.push_str(" = {};")
    }

    let mut output = fs::File::create(target_path).expect("Failed to create file");
    output
        .write_all(content.as_bytes())
        .expect("Failed to write to file");
}

fn write_runner(dir: &Path, context: &typecheck::Context, query_list: &ast::QueryList) {
    let target_path = dir.join("query.ts");
    let mut content = String::new();

    content.push_str("import { Config } from \"@libsql/client\";\n");
    content.push_str("import * as Db from \"./db\";\n");
    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                content.push_str(&format!(
                    "import * as {} from './query/{}';\n",
                    q.name,
                    crate::ext::string::decapitalize(&q.name)
                ));
            }
            _ => continue,
        }
    }
    content.push_str("\nexport const run = async (\n");
    content.push_str("  env: Config,\n");
    content.push_str("  id: string,\n");
    content.push_str("  args: any,\n");
    content.push_str("): Promise<Db.ExecuteResult> => {\n");
    content.push_str("    switch (id) {\n");

    for operation in &query_list.queries {
        match operation {
            ast::QueryDef::Query(q) => {
                content.push_str(&format!("        case \"{}\":\n", q.interface_hash));

                content.push_str(&format!(
                    "            return Db.run(env, {}.query, args);\n",
                    &q.name
                ));
            }
            _ => continue,
        }
    }
    content.push_str("        default:\n");
    content.push_str(
        "            return { kind: \"error\", errorType: Db.ErrorType.UnknownQuery, message: \"\" }\n"
    );

    content.push_str("    }\n");
    content.push_str("};\n");

    let mut output = fs::File::create(target_path).expect("Failed to create file");
    output
        .write_all(content.as_bytes())
        .expect("Failed to write to file");
}

pub fn literal_quote(s: &str) -> String {
    format!("`\n{}`", s)
}

fn format_ts_list(items: Vec<String>) -> String {
    let mut result = "[ ".to_string();
    let mut first = true;
    for item in items {
        if first {
            result.push_str(&format!("{}", item));
        } else {
            result.push_str(&format!(", {}", item));
        }

        first = false;
    }
    result.push_str("]");
    result
}

fn to_query_file(context: &typecheck::Context, query: &ast::Query) -> String {
    let mut result = "".to_string();
    result.push_str("import * as Ark from 'arktype';\n");
    result.push_str("import * as Db from '../db';\n");
    result.push_str("import * as Watched from '../watched';\n");
    result.push_str("import * as Decode from '../db/decode';\n\n");

    // Input args decoder
    to_query_input_decoder(context, &query, &mut result);

    result.push_str("\n\nconst sql = [");
    let mut watchers = vec![];

    let mut written_field = false;
    for field in &query.fields {
        let table = context.tables.get(&field.name).unwrap();

        for watched_operation in ast::to_watched_operations(table) {
            let name = format!("{}{}", table.name, operation_name(watched_operation));
            watchers.push(format!(
                "{{ kind: Watched.WatchedKind.{}, data: {{}} }}",
                name
            ));
        }

        if written_field {
            result.push_str(", ");
        }
        let sql = crate::generate::sql::to_string(context, query, &vec![field]);
        result.push_str(&literal_quote(&sql));
        written_field = true;
    }

    result.push_str("];\n\n\n");

    // Rectangle data decoder
    result.push_str("export const ReturnRectangle = Ark.type({\n");
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
    result.push_str("});\n\n");

    let validate = format!(
        r#"
export const query = Db.toRunner({{
    id: "{}",
    sql: sql,
    input: Input,
    output: ReturnRectangle,
    watch_triggers: {}
}});

type Input = typeof Input.infer
"#,
        query.interface_hash,
        format_ts_list(watchers)
    );

    result.push_str(&validate);

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

    result
}

fn to_query_input_decoder(context: &typecheck::Context, query: &ast::Query, result: &mut String) {
    result.push_str("export const Input = Ark.type({");
    for arg in &query.args {
        result.push_str(&format!(
            "\n  {}: {},",
            crate::ext::string::quote(&arg.name),
            to_ts_type_decoder(true, false, &arg.type_)
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
                to_ts_type_decoder(true, column.nullable, &column.type_)
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

fn format_db_id(table_alias: &str, ids: &Vec<String>) -> String {
    let mut result = String::new();
    for id in ids {
        let formatted = format!("{}__{}", table_alias, id);
        result.push_str(&format!("Db.Read.id \"{}\"", formatted));
    }
    result
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

fn to_nullable_type(is_nullable: bool, type_: &str) -> String {
    if is_nullable {
        format!("{} | null", type_)
    } else {
        type_.to_string()
    }
}

fn to_ts_type_decoder(qualified: bool, nullable: bool, type_: &str) -> String {
    match type_ {
        "String" => crate::ext::string::quote(&to_nullable_type(nullable, "string")),
        "Int" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        "Float" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        "Bool" => crate::ext::string::quote(&to_nullable_type(nullable, "boolean")),
        "DateTime" => crate::ext::string::quote(&to_nullable_type(nullable, "number")),
        _ => {
            let qualification = if qualified { "Decode." } else { "" };
            return to_nullable_type(nullable, &format!("{}{}", qualification, type_)).to_string();
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
        ast::QueryValue::Variable((range, name)) => format!("${}", name),
        ast::QueryValue::String((range, value)) => format!("\"{}\"", value),
        ast::QueryValue::Int((range, value)) => value.to_string(),
        ast::QueryValue::Float((range, value)) => value.to_string(),
        ast::QueryValue::Bool((range, value)) => value.to_string(),
        ast::QueryValue::Null(range) => "null".to_string(),
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

//
pub const DB_ENGINE: &str = r#"import {
  createClient,
  ResultSet,
  InStatement,
  InArgs,
  Config
} from "@libsql/client";
import * as Ark from "arktype";
import * as Watched from "./watched";

export type ExecuteResult = SuccessResult | ErrorResult;

export interface SuccessResult {
  kind: "success";
  metadata: {
    outOfDate: boolean;
    watched: Watched.Watched[];
  };
  data: ResultSet[];
}

export type ValidArgs = {
  kind: "valid";
  valid: InArgs;
};

export interface ErrorResult {
  kind: "error";
  errorType: ErrorType;
  message: string;
}

export enum ErrorType {
  NotFound,
  Unauthorized,
  InvalidInput,
  UnknownError,
  UnknownQuery
}

export interface Runner<input, output> {
  id: string;
  input: Ark.Type<input>;
  output: Ark.Type<output>;
  execute: (env: Config, args: ValidArgs) => Promise<ExecuteResult>;
}

export type ToRunnerArgs<input, output> = {
  id: string;
  input: Ark.Type<input>;
  output: Ark.Type<output>;
  sql: Array<string>;
  watch_triggers: Watched.Watched[];
};

export const toRunner = <Input, Output>(
  options: ToRunnerArgs<Input, Output>,
): Runner<Input, Output> => {
  return {
    id: options.id,
    input: options.input,
    output: options.output,
    execute: async (env: Config, args: ValidArgs): Promise<ExecuteResult> => {
      const sql_arg_list: InStatement[] = options.sql.map((sql) => {
        return { sql: sql, args: args.valid };
      });

      return exec(env, sql_arg_list, options.watch_triggers);
    },
  };
};

export const run = async (
  env: Config,
  runner: Runner<any, any>,
  args: any,
): Promise<ExecuteResult> => {
  const validArgs = validate(runner, args);
  if (validArgs.kind === "error") {
    return validArgs;
  }
  return runner.execute(env, validArgs);
};

const stringifyNestedObjects = (obj: Record<string, any>): Record<string, any> => {
    const result: Record<string, any> = {};

    for (const key in obj) {
        if (obj.hasOwnProperty(key)) {
            const value = obj[key];
            if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
                result[key] = JSON.stringify(value);
            } else {
                result[key] = value;
            }
        }
    }

    return result;
};

const validate = <Input extends InArgs, Output>(
  runner: Runner<Input, Output>,
  args: any,
): ErrorResult | ValidArgs => {

  const validationResult: any | Ark.ArkErrors = runner.input(args);

  if (validationResult instanceof Ark.type.errors) {
    return {
      kind: "error",
      errorType: ErrorType.InvalidInput,
      message: "Expected object",
    };
  } else {
    return { kind: "valid", valid: stringifyNestedObjects(validationResult) };
  }
};


// Queries

const exec = async (
  env: Config,
  sql: Array<InStatement>,
  watch_triggers: Watched.Watched[],
): Promise<ExecuteResult> => {
  const client = createClient(env);
  try {
    const res = await client.batch(sql);
    return {
      kind: "success",
      metadata: { outOfDate: false, watched: watch_triggers },
      data: res,
    };
  } catch (error) {
    console.log("DB ERROR", error)
    return {
      kind: "error",
      errorType: ErrorType.InvalidInput,
      message: "Expected object",
    };
  }
};
"#;
