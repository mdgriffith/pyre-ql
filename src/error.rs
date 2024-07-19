use crate::ast;
use colored::Colorize;
use nom::error::{VerboseError, VerboseErrorKind};
use nom::{Offset, ToUsize};
use nom_locate::LocatedSpan;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;

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

    // Query Validation Errors
    UnknownTable {
        found: String,
        existing: Vec<String>,
    },
    DuplicateQueryField {
        query: String,
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
        field: String,
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
    let description = to_error_description(&error);

    format!(
        "{} {}\n\n{}\n    {}",
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

fn render_expecting(expecting: &Expecting) -> String {
    match expecting {
        Expecting::PyreFile => "I ran into an issue parsing this that I didn't quite expect! I would love if you would file an issue on the repo showing the pyre file you're using.. ".to_string(),
        Expecting::ParamDefinition => return format!(
            "I was expecting a parameter, like:\n\n        {}\n\n    Hot tip: Running {} will automatically fix this for you.\n",
            "$id: Int".yellow(),
            "pyre format".cyan()
        ),
        Expecting::ParamDefType => return format!(
            "I was expecting a parameter type, like:\n\n        {}\n\n    Hot tip: Running {} will automatically fix this for you.\n",
            "$id: Int".yellow(),
            "pyre format".cyan()
        ),
        Expecting::AtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}\n        {}",
            "@where".yellow(),
            "@sort".yellow(),
            "@limit".yellow(),
            "@offset".yellow()
        ),
        Expecting::SchemaAtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}",
            "@watch".yellow(),
            "@tablename".yellow(),
            "@link".yellow()
        ),
        Expecting::SchemaFieldAtDirective => return format!(
            "I don't recognize this, did you mean one of these:\n\n        {}\n        {}\n        {}",
            "@id".yellow(),
            "@unique".yellow(),
            "@default".yellow()
        ),
        Expecting::LinkDirective => {
            let example =           format!("{} posts {{ from: id, to: Post.authorUserId }}", "@link".yellow());
            let example_breakdown =           format!("                            {} {}", "^^^^".cyan(), "^^^^^^^^^^^^".cyan());
            let example_breakdown_connector = format!("                            {}    {}", "|".cyan(), "|".cyan());
            let example_breakdown_labels =    format!("                {}    {}", "Foreign table".cyan(), "Foreign key".cyan());


            return format!(
                "This {} looks off, I'm expecting something that looks like this:\n\n        {}\n        {}\n        {}\n        {}",
                "@link".yellow(),
                example,
                example_breakdown,
                example_breakdown_connector,
                example_breakdown_labels
            )

        }


        // "I was expecting a link directive".to_string(),
    }
}

fn to_error_description(error: &Error) -> String {
    match &error.error_type {
        ErrorType::ParsingError(parsingDetails) => {
            let mut result = "".to_string();
            result.push_str(&format!("{}", render_expecting(&parsingDetails.expecting)));

            result.push_str("\n\n");
            result
        }

        ErrorType::MultipleSessionDeinitions => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I found multiple {} definitions, but there should only be one!",
                "session".cyan(),
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::UnknownFunction {
            found,
            known_functions,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I don't recognize this function: {}\n\n",
                found.cyan(),
            ));

            if known_functions.len() > 0 {
                result.push_str("\nHere are the functions I know:\n");
                for func in known_functions {
                    result.push_str(&format!("    {}\n", func.cyan()));
                }
            }

            result.push_str("\n\n");
            result
        }

        ErrorType::MissingType => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "All parameters need a type, like {}\n\n    Hot tip: Running {} will automatically fix this automatically for you!\n",
                "Int".yellow(),
                "pyre format".cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::DuplicateDefinition(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are two definitions for {}\n",
                name.yellow()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::DefinitionIsBuiltIn(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "The {} type is a built-in type, try using another name.\n",
                name.yellow()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::DuplicateField { record, field } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are multiple definitions for {} on {}.\n",
                field.yellow(),
                record.cyan()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::DuplicateVariant {
            base_variant,
            duplicates,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} has more than one variant named {}.\n",
                base_variant.typename.yellow(),
                base_variant.variant_name.cyan()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::DuplicateQueryField { query, field } => {
            let mut result = "".to_string();
            result.push_str(&format!("{} is listed multiple times.\n", field.yellow()));

            result.push_str("\n\n");
            result
        }
        ErrorType::UnknownTable { found, existing } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I don't recognize the '{}' table, is that a typo?\n",
                found.yellow()
            ));

            if existing.len() > 0 {
                result.push_str("\nThese tables might be similar\n");
                for table in existing {
                    result.push_str(&format!("    {}\n", table.cyan()));
                }
            }

            result.push_str("\n\n");
            result
        }

        ErrorType::LiteralTypeMismatch {
            expecting_type,
            found,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I was expecting {}, but found {}.\n",
                expecting_type.yellow(),
                found.cyan()
            ));

            result.push_str("\n\n");
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
                format!("${}", variable_name).yellow(),
                variable_defined_as.yellow(),
                column_defined_as.cyan()
            ));

            result.push_str("\n\n");
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
                link_name.yellow(),
                unknown_foreign_field.yellow(),
                foreign_table.yellow(),
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::LinkToUnknownField {
            link_name,
            unknown_local_field,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is trying to link using the {} column, but that column doesn't exist.",
                link_name.yellow(),
                unknown_local_field.yellow(),
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::LinkToUnknownTable {
            link_name,
            unknown_table,
        } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "{} is trying to link to the {} table, but that table doesn't exist.",
                link_name.yellow(),
                unknown_table.yellow(),
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::NoPrimaryKey { record } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} doesn't have a primary key, let's add one!",
                record.cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::MultiplePrimaryKeys { record, field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple primary keys, let's only have one.",
                record.cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::MultipleTableNames { record } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has has multiple @tablename definitions, let's only have one.",
                record.cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::MultipleLimits { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                query.cyan(),
                "@limits".yellow()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::MultipleOffsets { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                query.cyan(),
                "@offsets".yellow()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::MultipleWheres { query } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} has multiple {}, let's only have one!",
                query.cyan(),
                "@wheres".yellow()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::UndefinedParam { param, type_ } => {
            let mut result = "".to_string();
            let type_suggestion = match type_ {
                None => "".to_string(),
                Some(type_) => format!(
                    "\n    Add it to your declarations as {}: {}",
                    format!("${}", param).yellow(),
                    type_.cyan()
                ),
            };
            result.push_str(&format!(
                "{} is used, but not declared.{}",
                param.yellow(),
                type_suggestion
            ));

            result.push_str("\n\n");
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
                link_name.cyan(),
                foreign_table.cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::UnusedParam { param } => {
            let mut result = "".to_string();
            let colored_param = format!("${}", param).yellow();

            result.push_str(&format!(
                "{} isn't being used. Let's either use it or remove it.",
                colored_param
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::InsertColumnIsNotSet { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is required but not set to anything.",
                field.yellow(),
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::LinksDisallowedInDeletes { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which isn't allowed in a {}",
                field.yellow(),
                "@link".yellow(),
                "delete".cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::LinksDisallowedInUpdates { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which isn't allowed in a {}",
                field.yellow(),
                "@link".yellow(),
                "update".cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::LinksDisallowedInInserts { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which isn't allowed in a {}",
                field.yellow(),
                "@link".yellow(),
                "insert".cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::LimitOffsetOnlyInFlatRecord => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "This query has a limit/offset, but also queries nested values.\n\n{} isn't able to handle this situation yet and can only handle @limit and @offset in a query with no nested fields.",
                "Pyre".yellow(),

            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::NoSetsInSelect { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is being set, which isn't allowed in a {}",
                format!("${}", field).yellow(),
                "query".cyan()
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::NoSetsInDelete { field } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is being set, which isn't allowed in a {}",
                format!("${}", field).yellow(),
                "delete".cyan()
            ));

            result.push_str("\n\n");
            result
        }

        ErrorType::InsertMissingColumn { field } => {
            let mut result = "".to_string();

            result.push_str(&format!("This insert is missing {}", field.yellow()));

            result.push_str("\n\n");
            result
        }
        ErrorType::NoFieldsSelected => {
            let mut result = "".to_string();

            result.push_str("There are no fields selected for this table, let's add some!");

            result.push_str("\n\n");
            result
        }
        ErrorType::WhereOnLinkIsntAllowed { link_name } => {
            let mut result = "".to_string();

            result.push_str(&format!(
                "{} is a {}, which can't be in a {}",
                link_name.cyan(),
                "@link".cyan(),
                "@where".yellow(),
            ));

            result.push_str("\n\n");
            result
        }
        ErrorType::UnknownType { found, known_types } => {
            let mut result = "".to_string();
            let colored_param = format!("{}", found).cyan();

            result.push_str(&format!(
                "I don't recognize the {} type, is that a typo?",
                colored_param
            ));

            result.push_str("\n\n    Here are the types I know:\n\n");

            let mut sorted_types: Vec<String> = known_types.clone();
            sorted_types.sort();
            for typename in sorted_types {
                result.push_str(&format!("        {}\n", typename.cyan()));
            }

            result.push_str("\n\n");
            result
        }

        ErrorType::UnknownField {
            found,
            record_name,
            known_fields,
        } => {
            let mut result = "".to_string();
            let colored_param = format!("{}", found).yellow();
            let mut a_or_an = "a";

            if found.starts_with('a') {
                a_or_an = "an";
            }

            result.push_str(&format!(
                "{} doesn't have {} {} field, is that a typo?",
                record_name.cyan(),
                a_or_an,
                colored_param
            ));

            result.push_str(&format!(
                "\n\n    Here are the fields on {}:\n\n",
                record_name.cyan()
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
                    field_name.yellow(),
                    spacing,
                    field_type.cyan()
                ));
            }

            result.push_str("\n\n");
            result
        }
    }
}
