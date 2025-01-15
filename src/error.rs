use crate::ast;
use colored::Colorize;
use nom::ToUsize;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize, Serialize)]
pub struct Error {
    pub error_type: ErrorType,
    pub filepath: String,
    pub locations: Vec<Location>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ParsingErrorDetails {
    pub expecting: Expecting,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum Expecting {
    PyreFile,
    // Query stuff
    ParamDefinition,
    ParamDefType,
    AtDirective,

    // Schema stuff
    SchemaAtDirective,
    SchemaFieldAtDirective,
    SchemaColumn,

    LinkDirective,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ErrorType {
    ParsingError(ParsingErrorDetails),

    UnknownFunction {
        found: String,
        known_functions: Vec<String>,
    },
    MultipleSessionDeinitions,
    MissingType,
    DuplicateDefinition(String),
    DefinitionIsBuiltIn(String),
    DuplicateField {
        record: String,
        field: String,
    },
    DuplicateVariant {
        base_variant: VariantDef,
        duplicates: Vec<VariantDef>,
    },
    UnknownType {
        found: String,
        known_types: Vec<String>,
    },
    NoPrimaryKey {
        record: String,
    },
    MultiplePrimaryKeys {
        record: String,
        field: String,
    },
    MultipleTableNames {
        record: String,
    },
    // Schema Link errors
    LinkToUnknownTable {
        link_name: String,
        unknown_table: String,
    },

    LinkToUnknownField {
        link_name: String,
        unknown_local_field: String,
    },
    LinkToUnknownForeignField {
        link_name: String,
        foreign_table: String,
        unknown_foreign_field: String,
    },
    LinkSelectionIsEmpty {
        link_name: String,
        foreign_table: String,
        foreign_table_fields: Vec<(String, String)>,
    },
    LinkToUnknownSchema {
        unknown_schema_name: String,
        known_schemas: HashSet<String>,
    },

    // Query Validation Errors
    UnknownTable {
        found: String,
        existing: Vec<String>,
    },
    DuplicateQueryField {
        field: String,
    },
    NoFieldsSelected,
    UnknownField {
        found: String,

        record_name: String,
        known_fields: Vec<(String, String)>,
    },
    MultipleLimits {
        query: String,
    },
    MultipleOffsets {
        query: String,
    },
    MultipleWheres {
        query: String,
    },
    WhereOnLinkIsntAllowed {
        link_name: String,
    },
    TypeMismatch {
        table: String,
        column_defined_as: String,
        variable_name: String,
        variable_defined_as: String,
    },
    LiteralTypeMismatch {
        expecting_type: String,
        found: String,
    },
    UnusedParam {
        param: String,
    },
    UndefinedParam {
        param: String,
        type_: Option<String>,
    },
    NoSetsInSelect {
        field: String,
    },
    NoSetsInDelete {
        field: String,
    },
    LinksDisallowedInInserts {
        field: String,
        table_name: String,
        local_ids: Vec<String>,
    },
    LinksDisallowedInDeletes {
        field: String,
    },
    LinksDisallowedInUpdates {
        field: String,
    },
    InsertColumnIsNotSet {
        field: String,
    },
    InsertMissingColumn {
        fields: Vec<String>,
    },
    InsertNestedValueAutomaticallySet {
        field: String,
    },
    MultipleSchemaWrites {
        field_table: String,
        field_schema: String,
        operation: ast::QueryOperation,
        other_schemas: Vec<String>,
    },
    LimitOffsetOnlyInFlatRecord,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum DefInfo {
    Def(Option<Range>),
    Builtin,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VariantDef {
    pub typename: String,
    pub variant_name: String,
    pub range: Option<Range>,
}

/*


    For tracking location errors, we have a few different considerations.

    1. Generally a language server takes a single range, so that should easily be retrievable.
    2. For error rendering in the terminal, we want a hierarchy of the contexts we're in.
        So, we want
            - The Query
            - The table field, etc.

*/

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Location {
    pub contexts: Vec<Range>,
    pub primary: Vec<Range>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Range {
    pub start: ast::Location,
    pub end: ast::Location,
}

pub fn format_custom_error(title: &str, body: &str) -> String {
    format!(
        "{}-------------{}\n\n{}",
        title,
        "-".repeat(title.len()),
        body
    )
}

/* Error formats!



{File name}-------------{Error title}

   | record User {
   |    ...
12 |    status: Stats
   |            ^^^^^
   | }

I don't recognize this type. Is it one of these?

   Status




*/

pub fn format_error(file_contents: &str, error: &Error) -> String {
    let filepath = &error.filepath;
    let path_length = filepath.len();
    let separator = "-".repeat(50 - path_length);

    let highlight = prepare_highlight(file_contents, &error);
    let description = to_error_description(&error, true);

    format!(
        "{} {}\n\n{}\n    {}\n",
        filepath.cyan(),
        separator.cyan(),
        highlight,
        description
    )
}

fn prepare_highlight(file_contents: &str, error: &Error) -> String {
    let mut rendered = "".to_string();
    let mut has_rendered = false;
    for location in &error.locations {
        if has_rendered {
            rendered.push_str("\n\n");
        }
        render_highlight_location(file_contents, &mut rendered, &location);
        has_rendered = true;
    }
    rendered
}

fn divider(indent: usize) -> String {
    format!("    | {}...\n", " ".repeat(indent * 4))
        .truecolor(120, 120, 120)
        .to_string()
}

fn join_hashset(set: &HashSet<String>, sep: &str) -> String {
    set.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(sep)
}

fn render_highlight_location(file_contents: &str, rendered: &mut String, location: &Location) {
    let mut indent: usize = 0;

    let mut last_line_index: usize = 0;
    let mut first_rendered = false;

    for context in &location.contexts {
        if first_rendered && context.start.line.to_usize() > last_line_index + 1 {
            rendered.push_str(&divider(indent))
        }
        rendered.push_str(&get_line(&file_contents, false, context.start.line));
        rendered.push_str("\n");

        first_rendered = true;
        last_line_index = context.start.line.to_usize();
        indent += 1;
    }
    let mut first_primary_rendered = false;
    for primary in &location.primary {
        if primary.start.line.to_usize() > last_line_index + 1
            && (first_rendered || first_primary_rendered)
        {
            rendered.push_str(&divider(indent))
        }

        if primary.start.line == primary.end.line {
            rendered.push_str(&get_line(file_contents, true, primary.start.line));
            rendered.push_str("\n");
            rendered.push_str(&highlight_line(&primary));
            rendered.push_str("\n");
        } else {
            rendered.push_str(&get_lines(
                file_contents,
                true,
                primary.start.line,
                primary.end.line,
            ));
            rendered.push_str("\n");
        }

        last_line_index = primary.end.line.to_usize();
        first_primary_rendered = true;
    }

    for context in location.contexts.iter().rev() {
        if last_line_index == context.end.line.to_usize() {
            continue;
        }
        if context.end.line.to_usize() > last_line_index + 1 {
            rendered.push_str(&divider(indent))
        }

        rendered.push_str(&get_line(&file_contents, false, context.end.line));
        rendered.push_str("\n");

        last_line_index = context.end.line.to_usize();
        indent -= 1;
    }
}

fn line_number_prefix(show_line_number: bool, number: usize) -> String {
    if show_line_number {
        let num = number.to_string();
        if number < 10 {
            format!("   {}| ", num)
        } else if number < 100 {
            format!("  {}| ", num)
        } else if number < 1000 {
            format!(" {}| ", num)
        } else {
            format!("{}| ", num)
        }
    } else {
        "    | ".to_string()
    }
}

fn highlight_line(range: &Range) -> String {
    if range.start.column < range.end.column && range.start.line == range.end.line {
        let indent = " ".repeat(range.start.column);
        let highlight = "^".repeat(range.end.column - range.start.column);
        format!(
            "    {}{}{}",
            "|".truecolor(120, 120, 120),
            indent,
            highlight.red()
        )
    } else if range.start.column == range.end.column && range.start.line == range.end.line {
        let indent = " ".repeat(range.start.column);
        let highlight = "^";
        format!(
            "    {}{}{}",
            "|".truecolor(120, 120, 120),
            indent,
            highlight.red()
        )
    } else {
        println!("CROSSED RANGE {:#?}", range);
        "    ^^".red().to_string()
    }
}

fn get_line(file_contents: &str, show_line_number: bool, line_index: u32) -> String {
    let line_number = line_index.to_usize() - 1;

    let prefix =
        line_number_prefix(show_line_number, line_index.to_usize()).truecolor(120, 120, 120);

    for (index, line) in file_contents.to_string().lines().enumerate() {
        if line_number == index {
            return format!("{}{}", prefix, line.to_string());
        }
    }
    prefix.to_string()
}

fn get_lines(file_contents: &str, show_line_number: bool, start: u32, end: u32) -> String {
    let start_line_number = start.to_usize() - 1;
    let end_line_number = end.to_usize() - 1;

    let mut result = "".to_string();

    for (index, line) in file_contents.to_string().lines().enumerate() {
        if start_line_number <= index && end_line_number >= index {
            let prefix =
                line_number_prefix(show_line_number, index.to_usize() + 1).truecolor(120, 120, 120);
            result.push_str(&format!("{}{}", prefix, line.to_string()));
            result.push_str("\n");
        }
    }
    result
}

fn render_expecting(expecting: &Expecting, in_color: bool) -> String {
    match expecting {
        Expecting::PyreFile => "I ran into an issue parsing this that I didn't quite expect! I would love if you would file an issue on the repo showing the pyre file you're using.. ".to_string(),
        Expecting::ParamDefinition => return format!(
            "I was expecting a parameter, like:\n\n        {}\n\n    Hot tip: Running {} will automatically fix this for you.\n",
            yellow_if(in_color, "$id: Int"),
            cyan_if(in_color, "pyre format")
        ),
        Expecting::ParamDefType => return format!(
            "I was expecting a parameter type, like:\n\n        {}\n\n    Hot tip: Running {} will automatically fix this for you.\n",
            yellow_if(in_color, "$id: Int"),
            cyan_if(in_color, "pyre format")
        ),
        Expecting::AtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}\n        {}",
            yellow_if(in_color, "@where"),
            yellow_if(in_color, "@sort"),
            yellow_if(in_color, "@limit"),
            yellow_if(in_color, "@offset")
        ),
        Expecting::SchemaAtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}",
            yellow_if(in_color, "@watch"),
            yellow_if(in_color, "@tablename"),
            yellow_if(in_color, "@link")
        ),
        Expecting::SchemaFieldAtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}",
            yellow_if(in_color, "@id"),
            yellow_if(in_color, "@unique"),
            yellow_if(in_color, "@default")
        ),
        Expecting::SchemaColumn => return format!(
            "I was expecting a column, like:\n\n        {}",
            yellow_if(in_color, "id    Int")
        ),
        Expecting::LinkDirective => {
            let example = format!("{} posts {{ from: id, to: Post.authorUserId }}", 
                yellow_if(in_color, "@link"));
            let example_breakdown = format!("                            {} {}", 
                cyan_if(in_color, "^^^^"), 
                cyan_if(in_color, "^^^^^^^^^^^^"));
            let example_breakdown_connector = format!("                            {}    {}", 
                cyan_if(in_color, "|"), 
                cyan_if(in_color, "|"));
            let example_breakdown_labels = format!("                {}    {}", 
                cyan_if(in_color, "Foreign table"), 
                cyan_if(in_color, "Foreign key"));

            return format!(
                "This {} looks off, I'm expecting something that looks like this:\n\n        {}\n        {}\n        {}\n        {}",
                yellow_if(in_color, "@link"),
                example,
                example_breakdown,
                example_breakdown_connector,
                example_breakdown_labels
            )
        }


        // "I was expecting a link directive".to_string(),
    }
}

pub fn cyan_if(in_color: bool, text: &str) -> String {
    if in_color {
        text.cyan().to_string()
    } else {
        text.to_string()
    }
}

pub fn yellow_if(in_color: bool, text: &str) -> String {
    if in_color {
        text.yellow().to_string()
    } else {
        text.to_string()
    }
}

pub fn format_yellow_list(in_color: bool, items: Vec<String>) -> String {
    let mut result = "".to_string();
    for item in items {
        result.push_str(&format!("    {}\n", yellow_if(in_color, &item)));
    }
    result
}

fn format_yellow_or_list(items: &Vec<String>, in_color: bool) -> String {
    match items.len() {
        0 => String::new(),
        1 => yellow_if(in_color, &items[0]),
        2 => format!(
            "{} or {}",
            yellow_if(in_color, &items[0]),
            yellow_if(in_color, &items[1])
        ),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!(
                "{}, or {}",
                rest.iter()
                    .map(|item| yellow_if(in_color, item))
                    .collect::<Vec<_>>()
                    .join(", "),
                yellow_if(in_color, last)
            )
        }
    }
}

fn to_error_description(error: &Error, in_color: bool) -> String {
    match &error.error_type {
        ErrorType::ParsingError(parsing_details) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{}",
                render_expecting(&parsing_details.expecting, in_color)
            ));

            result
        }

        ErrorType::MultipleSessionDeinitions => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I found multiple {} definitions, but there should only be one!",
                cyan_if(in_color, "session"),
            ));

            result
        }

        ErrorType::UnknownFunction {
            found,
            known_functions,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I don't recognize this function: {}\n\n",
                cyan_if(in_color, found),
            ));

            if known_functions.len() > 0 {
                result.push_str("\nHere are the functions I know:\n");
                for func in known_functions {
                    result.push_str(&format!("    {}\n", cyan_if(in_color, func)));
                }
            }

            result
        }

        ErrorType::MissingType => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "All parameters need a type, like {}\n\n    Hot tip: Running {} will automatically fix this automatically for you!\n",
                yellow_if(in_color, "Int"),
                cyan_if(in_color, "pyre format")
            ));

            result
        }

        ErrorType::DuplicateDefinition(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are two definitions for {}\n",
                yellow_if(in_color, name)
            ));

            result
        }

        ErrorType::DefinitionIsBuiltIn(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "The {} type is a built-in type, try using another name.\n",
                yellow_if(in_color, name)
            ));

            result
        }
        ErrorType::DuplicateField { record, field } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are multiple definitions for {} on {}.\n",
                yellow_if(in_color, field),
                cyan_if(in_color, record)
            ));

            result
        }
        ErrorType::DuplicateVariant {
            base_variant,
            duplicates,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} has more than one variant named {}.\n",
                yellow_if(in_color, &base_variant.typename),
                cyan_if(in_color, &base_variant.variant_name)
            ));

            result
        }
        ErrorType::DuplicateQueryField { field } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is listed multiple times.\n",
                yellow_if(in_color, field)
            ));

            result
        }
        ErrorType::UnknownTable { found, existing } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I don't recognize the '{}' table, is that a typo?\n",
                yellow_if(in_color, found)
            ));

            if existing.len() > 0 {
                result.push_str("\nThese tables might be similar\n");
                for table in existing {
                    result.push_str(&format!("    {}\n", cyan_if(in_color, table)));
                }
            }

            result
        }

        ErrorType::LiteralTypeMismatch {
            expecting_type,
            found,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I was expecting {}, but found {}.\n",
                yellow_if(in_color, expecting_type),
                cyan_if(in_color, found)
            ));

            result
        }

        ErrorType::TypeMismatch {
            table,
            column_defined_as,
            variable_name,
            variable_defined_as,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is defined as {}, but I'm expecting a {}.\n",
                yellow_if(in_color, &format!("${}", variable_name)),
                yellow_if(in_color, variable_defined_as),
                cyan_if(in_color, column_defined_as)
            ));

            result
        }

        ErrorType::LinkToUnknownForeignField {
            link_name,
            foreign_table,
            unknown_foreign_field,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is trying to link to the {} column on the {} table, but that column doesn't exist.\n",
                yellow_if(in_color, link_name),
                yellow_if(in_color, unknown_foreign_field),
                yellow_if(in_color, foreign_table),
            ));

            result
        }

        ErrorType::LinkToUnknownField {
            link_name,
            unknown_local_field,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is trying to link using the {} column, but that column doesn't exist.",
                yellow_if(in_color, link_name),
                yellow_if(in_color, unknown_local_field),
            ));

            result
        }
        ErrorType::LinkToUnknownTable {
            link_name,
            unknown_table,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is trying to link to the {} table, but that table doesn't exist.",
                yellow_if(in_color, link_name),
                yellow_if(in_color, unknown_table),
            ));

            result
        }

        ErrorType::NoPrimaryKey { record } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} doesn't have a primary key, let's add one!",
                cyan_if(in_color, record)
            ));

            result
        }

        ErrorType::MultiplePrimaryKeys { record, field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple primary keys, let's only have one.",
                cyan_if(in_color, record)
            ));

            result
        }

        ErrorType::MultipleTableNames { record } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has has multiple @tablename definitions, let's only have one.",
                cyan_if(in_color, record)
            ));

            result
        }

        ErrorType::MultipleLimits { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                cyan_if(in_color, query),
                yellow_if(in_color, "@limits")
            ));

            result
        }
        ErrorType::MultipleOffsets { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                cyan_if(in_color, query),
                yellow_if(in_color, "@offsets")
            ));

            result
        }
        ErrorType::MultipleWheres { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                cyan_if(in_color, query),
                yellow_if(in_color, "@wheres")
            ));

            result
        }
        ErrorType::UndefinedParam { param, type_ } => {
            let mut result = "".to_string();
            let type_suggestion = match type_ {
                None => "".to_string(),
                Some(type_) => format!(
                    "\nAdd it to your declarations as {}: {}",
                    yellow_if(in_color, param),
                    cyan_if(in_color, type_)
                ),
            };
            result.push_str(&format!(
                "{} is used, but not declared.{}",
                yellow_if(in_color, param),
                type_suggestion
            ));

            result
        }
        ErrorType::LinkSelectionIsEmpty {
            link_name,
            foreign_table,
            foreign_table_fields,
        } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a link to the {} table, but doesn't select any fields.  Let's select some!",
                cyan_if(in_color, link_name),
                cyan_if(in_color, foreign_table)
            ));

            result
        }

        ErrorType::LinkToUnknownSchema {
            unknown_schema_name,
            known_schemas,
        } => {
            if known_schemas.len() == 1 {
                let mut result = "".to_string();

                result.push_str(&format!(
                    "I don't recognize {} as a schema. There's only one schema, {}, so nothing should be qualified.",
                    cyan_if(in_color, unknown_schema_name),
                    cyan_if(in_color, &join_hashset(known_schemas, "\n    "))
                ));

                result
            } else {
                let mut result = "".to_string();

                result.push_str(&format!(
                    "I don't recognize {} as a schema. Here are the schemas I know about:\n{}",
                    cyan_if(in_color, unknown_schema_name),
                    cyan_if(in_color, &join_hashset(known_schemas, "\n    "))
                ));

                result
            }
        }

        ErrorType::UnusedParam { param } => {
            let mut result = "".to_string();
            let colored_param = yellow_if(in_color, param);

            result.push_str(&format!(
                "{} isn't being used. Let's either use it or remove it.",
                colored_param
            ));

            result
        }
        ErrorType::InsertColumnIsNotSet { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is required but not set to anything.",
                yellow_if(in_color, field),
            ));

            result
        }
        ErrorType::LinksDisallowedInDeletes { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which isn't allowed in a {}",
                yellow_if(in_color, field),
                yellow_if(in_color, "@link"),
                cyan_if(in_color, "delete")
            ));

            result
        }

        ErrorType::LinksDisallowedInUpdates { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which isn't allowed in a {}",
                yellow_if(in_color, field),
                yellow_if(in_color, "@link"),
                cyan_if(in_color, "update")
            ));

            result
        }

        ErrorType::LinksDisallowedInInserts {
            field,
            table_name,
            local_ids,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "Nested inserts are only allowed if you start with a primary key.\n\n{} links via {}, which isn't the primary key of the {} table.",
                yellow_if(in_color, field),
                yellow_if(in_color, &local_ids.clone().join(", ")),
                yellow_if(in_color, table_name),
            ));

            result
        }

        ErrorType::LimitOffsetOnlyInFlatRecord => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "This query has a limit/offset, but also queries nested values.\n\n{} isn't able to handle this situation yet and can only handle @limit and @offset in a query with no nested fields.",
                yellow_if(in_color, "Pyre"),
            ));

            result
        }

        ErrorType::NoSetsInSelect { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is being set, which isn't allowed in a {}",
                yellow_if(in_color, &format!("${}", field)),
                cyan_if(in_color, "query")
            ));

            result
        }
        ErrorType::NoSetsInDelete { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is being set, which isn't allowed in a {}",
                yellow_if(in_color, &format!("${}", field)),
                cyan_if(in_color, "delete")
            ));

            result
        }

        ErrorType::InsertMissingColumn { fields } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "This insert is missing {}",
                yellow_if(in_color, &fields.join(", "))
            ));

            result
        }
        ErrorType::InsertNestedValueAutomaticallySet { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "Pyre is setting {} automatically based on your {}, no need to set it manually.",
                yellow_if(in_color, field),
                yellow_if(in_color, "@link")
            ));

            result
        }
        ErrorType::MultipleSchemaWrites {
            field_table,
            field_schema,
            operation,
            other_schemas,
        } => {
            let mut result = "".to_string();

            let operation_words = match operation {
                ast::QueryOperation::Select => "selecting from",
                ast::QueryOperation::Insert => "inserting a value to",
                ast::QueryOperation::Update => "updating a value on",
                ast::QueryOperation::Delete => "deleting from",
            };

            let schema_words: String = format_yellow_or_list(&other_schemas, in_color);

            result.push_str(&format!(
                "This value is on the {} table and is {} the {} schema, but you can only write to one schema in a query. Everything else is writing to {}",
                yellow_if(in_color, field_table),
                operation_words,
                yellow_if(in_color, field_schema),
                schema_words
            ));

            result
        }
        ErrorType::NoFieldsSelected => {
            let mut result = "".to_string();

            result.push_str("There are no fields selected for this table, let's add some!");

            result
        }
        ErrorType::WhereOnLinkIsntAllowed { link_name } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which can't be in a {}",
                cyan_if(in_color, link_name),
                cyan_if(in_color, "@link"),
                yellow_if(in_color, "@where"),
            ));

            result
        }
        ErrorType::UnknownType { found, known_types } => {
            let mut result = "".to_string();
            let colored_param = cyan_if(in_color, found);

            result.push_str(&format!(
                "I don't recognize the {} type, is that a typo?",
                colored_param
            ));

            result.push_str("\n\n    Here are the types I know:\n\n");

            let mut sorted_types: Vec<String> = known_types.clone();
            sorted_types.sort();
            for typename in sorted_types {
                result.push_str(&format!("        {}\n", cyan_if(in_color, &typename)));
            }

            result
        }

        ErrorType::UnknownField {
            found,
            record_name,
            known_fields,
        } => {
            let mut result = "".to_string();
            let colored_param = yellow_if(in_color, found);
            let mut a_or_an = "a";

            if found.starts_with('a') {
                a_or_an = "an";
            }

            result.push_str(&format!(
                "{} doesn't have {} {} field, is that a typo?",
                cyan_if(in_color, record_name),
                a_or_an,
                colored_param
            ));

            result.push_str(&format!(
                "\n\n    Here are the fields on {}:\n\n",
                cyan_if(in_color, record_name)
            ));
            let mut largest_fieldname_size: usize = 0;
            for (field_name, _) in known_fields {
                let len = field_name.len();
                if len > largest_fieldname_size {
                    largest_fieldname_size = len
                }
            }

            for (field_name, field_type) in known_fields {
                let extra_spacing_amount = (largest_fieldname_size - field_name.len()) + 1;
                let spacing = " ".repeat(extra_spacing_amount);
                result.push_str(&format!(
                    "        {}:{}{}\n",
                    yellow_if(in_color, &field_name),
                    spacing,
                    cyan_if(in_color, &field_type)
                ));
            }

            result
        }
    }
}

// JSON error format
fn to_error_title(error_type: &ErrorType) -> String {
    match error_type {
        ErrorType::ParsingError(_) => "Parsing Error",
        ErrorType::UnknownFunction { .. } => "Unknown Function",
        ErrorType::MultipleSessionDeinitions => "Multiple Session Definitions",
        ErrorType::MissingType => "Missing Type",
        ErrorType::DuplicateDefinition(_) => "Duplicate Definition",
        ErrorType::DefinitionIsBuiltIn(_) => "Definition Is Built-in",
        ErrorType::DuplicateField { .. } => "Duplicate Field",
        ErrorType::DuplicateVariant { .. } => "Duplicate Variant",
        ErrorType::UnknownType { .. } => "Unknown Type",
        ErrorType::NoPrimaryKey { .. } => "No Primary Key",
        ErrorType::MultiplePrimaryKeys { .. } => "Multiple Primary Keys",
        ErrorType::MultipleTableNames { .. } => "Multiple table names",
        ErrorType::LinkToUnknownTable { .. } => "Link to unknown table",
        ErrorType::LinkToUnknownField { .. } => "Link to unknown field",
        ErrorType::LinkToUnknownForeignField { .. } => "Link to Unknown Foreign Field",
        ErrorType::LinkSelectionIsEmpty { .. } => "Link Selection Is Empty",
        ErrorType::LinkToUnknownSchema { .. } => "Link to Unknown Schema",
        ErrorType::UnknownTable { .. } => "Unknown Table",
        ErrorType::DuplicateQueryField { .. } => "Duplicate Query Field",
        ErrorType::NoFieldsSelected => "No Fields Selected",
        ErrorType::UnknownField { .. } => "Unknown Field",
        ErrorType::MultipleLimits { .. } => "Multiple Limits",
        ErrorType::MultipleOffsets { .. } => "Multiple Offsets",
        ErrorType::MultipleWheres { .. } => "Multiple Wheres",
        ErrorType::WhereOnLinkIsntAllowed { .. } => "Where On Link Not Allowed",
        ErrorType::TypeMismatch { .. } => "Type Mismatch",
        ErrorType::LiteralTypeMismatch { .. } => "Literal Type Mismatch",
        ErrorType::UnusedParam { .. } => "Unused Parameter",
        ErrorType::UndefinedParam { .. } => "Undefined Parameter",
        ErrorType::NoSetsInSelect { .. } => "No Sets In Select",
        ErrorType::NoSetsInDelete { .. } => "No Sets In Delete",
        ErrorType::LinksDisallowedInInserts { .. } => "Links Not Allowed In Inserts",
        ErrorType::LinksDisallowedInDeletes { .. } => "Links Not Allowed In Deletes",
        ErrorType::LinksDisallowedInUpdates { .. } => "Links Not Allowed In Updates",
        ErrorType::InsertColumnIsNotSet { .. } => "Insert Column Not Set",
        ErrorType::InsertMissingColumn { .. } => "Insert Missing Column",
        ErrorType::InsertNestedValueAutomaticallySet { .. } => "Can't set automatic field",
        ErrorType::MultipleSchemaWrites { .. } => "Multiple Schema Writes",
        ErrorType::LimitOffsetOnlyInFlatRecord => "Limit/Offset Only In Flat Record",
    }
    .to_string()
}

pub fn format_json(error: &Error) -> serde_json::Value {
    let mut error_json = serde_json::Map::new();

    let title = to_error_title(&error.error_type);
    let description = to_error_description(&error, false);

    // Add filepath
    error_json.insert(
        "filepath".to_string(),
        serde_json::Value::String(error.filepath.clone()),
    );

    // Add locations
    let mut locations = Vec::new();
    for location in &error.locations {
        let mut location_json = serde_json::Map::new();

        // Add primary ranges
        let mut primary_ranges = Vec::new();
        for range in &location.primary {
            let mut range_json = serde_json::Map::new();

            // Create start object
            let mut start = serde_json::Map::new();
            start.insert(
                "line".to_string(),
                serde_json::Value::Number(range.start.line.into()),
            );
            start.insert(
                "column".to_string(),
                serde_json::Value::Number(range.start.column.into()),
            );
            range_json.insert("start".to_string(), serde_json::Value::Object(start));

            // Create end object
            let mut end = serde_json::Map::new();
            end.insert(
                "line".to_string(),
                serde_json::Value::Number(range.end.line.into()),
            );
            end.insert(
                "column".to_string(),
                serde_json::Value::Number(range.end.column.into()),
            );
            range_json.insert("end".to_string(), serde_json::Value::Object(end));

            primary_ranges.push(serde_json::Value::Object(range_json));
        }
        location_json.insert(
            "primary".to_string(),
            serde_json::Value::Array(primary_ranges),
        );

        // Add context ranges
        let mut context_ranges = Vec::new();
        for range in &location.contexts {
            let mut range_json = serde_json::Map::new();

            // Create start object
            let mut start = serde_json::Map::new();
            start.insert(
                "line".to_string(),
                serde_json::Value::Number(range.start.line.into()),
            );
            start.insert(
                "column".to_string(),
                serde_json::Value::Number(range.start.column.into()),
            );
            range_json.insert("start".to_string(), serde_json::Value::Object(start));

            // Create end object
            let mut end = serde_json::Map::new();
            end.insert(
                "line".to_string(),
                serde_json::Value::Number(range.end.line.into()),
            );
            end.insert(
                "column".to_string(),
                serde_json::Value::Number(range.end.column.into()),
            );
            range_json.insert("end".to_string(), serde_json::Value::Object(end));

            context_ranges.push(serde_json::Value::Object(range_json));
        }
        location_json.insert(
            "contexts".to_string(),
            serde_json::Value::Array(context_ranges),
        );

        locations.push(serde_json::Value::Object(location_json));
    }
    error_json.insert("locations".to_string(), serde_json::Value::Array(locations));

    error_json.insert("title".to_string(), serde_json::Value::String(title));
    error_json.insert(
        "description".to_string(),
        serde_json::Value::String(description),
    );

    serde_json::Value::Object(error_json)
}

pub fn format_libsql_error(e: &libsql::Error) -> String {
    match e {
        libsql::Error::ConnectionFailed(s) => format_custom_error("Connection Failed", s),
        libsql::Error::SqliteFailure(_, s) => format_custom_error("SQLite Failure", s),
        libsql::Error::NullValue => format_custom_error("Null Value", "Null value encountered"),
        libsql::Error::Misuse(s) => format_custom_error("API Misuse", s),
        libsql::Error::ExecuteReturnedRows => {
            format_custom_error("Execute Returned Rows", "Execute returned rows")
        }
        libsql::Error::QueryReturnedNoRows => {
            format_custom_error("Query Returned No Rows", "Query returned no rows")
        }
        libsql::Error::InvalidColumnName(s) => format_custom_error("Invalid Column Name", s),
        libsql::Error::ToSqlConversionFailure(e) => {
            format_custom_error("SQL Conversion Failure", &format!("{}", e))
        }
        libsql::Error::SyncNotSupported(s) => format_custom_error("Sync Not Supported", s),
        libsql::Error::ColumnNotFound(_) => {
            format_custom_error("Column Not Found", "Column not found")
        }
        libsql::Error::Hrana(e) => format_custom_error("Hrana", &format!("{}", e)),
        libsql::Error::WriteDelegation(e) => {
            format_custom_error("Write Delegation", &format!("{}", e))
        }
        libsql::Error::Bincode(e) => format_custom_error("Bincode", &format!("{}", e)),
        libsql::Error::InvalidColumnIndex => {
            format_custom_error("Invalid Column Index", "Invalid column index")
        }
        libsql::Error::InvalidColumnType => {
            format_custom_error("Invalid Column Type", "Invalid column type")
        }
        libsql::Error::Sqlite3SyntaxError(_, _, s) => {
            format_custom_error("SQLite3 Syntax Error", s)
        }
        libsql::Error::Sqlite3UnsupportedStatement => {
            format_custom_error("SQLite3 Unsupported Statement", "Unsupported statement")
        }
        libsql::Error::Sqlite3ParserError(e) => {
            format_custom_error("SQLite3 Parser Error", &format!("{}", e))
        }
        libsql::Error::RemoteSqliteFailure(_, _, s) => {
            format_custom_error("Remote SQLite Failure", s)
        }
        libsql::Error::Replication(e) => format_custom_error("Replication", &format!("{}", e)),
        libsql::Error::InvalidUTF8Path => {
            format_custom_error("Invalid UTF-8 Path", "Path has invalid UTF-8")
        }
        libsql::Error::FreezeNotSupported(s) => format_custom_error("Freeze Not Supported", s),
        libsql::Error::InvalidParserState(s) => format_custom_error("Invalid Parser State", s),
        libsql::Error::InvalidTlsConfiguration(e) => {
            format_custom_error("Invalid TLS Configuration", &format!("{}", e))
        }
    }
}
