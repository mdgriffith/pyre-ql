use crate::typecheck;
use colored::Colorize;
use nom::error::{Error, VerboseError, VerboseErrorKind};
use nom::{Offset, ToUsize};
use nom_locate::LocatedSpan;
use std::fmt::Write;

/// Transforms a `VerboseError` into a trace with input position information

pub fn convert_error(input: LocatedSpan<&str>, error: VerboseError<LocatedSpan<&str>>) -> String {
    let mut result = String::new();

    for (i, (substring, kind)) in error.errors.iter().enumerate() {
        println!("{:#?}", (kind));
        let offset = input.offset(substring);

        if input.is_empty() {
            match kind {
                VerboseErrorKind::Char(c) => {
                    write!(&mut result, "{}: expected '{}', got empty input\n\n", i, c)
                }
                VerboseErrorKind::Context(s) => {
                    write!(&mut result, "{}: in {}, got empty input\n\n", i, s)
                }
                VerboseErrorKind::Nom(e) => {
                    write!(&mut result, "{}: in {:?}, got empty input\n\n", i, e)
                }
            }
        } else {
            let prefix = &input.as_bytes()[..offset];

            // Count the number of newlines in the first `offset` bytes of input
            let line_number = prefix.iter().filter(|&&b| b == b'\n').count() + 1;

            // Find the line that includes the subslice:
            // Find the *last* newline before the substring starts
            let line_begin = prefix
                .iter()
                .rev()
                .position(|&b| b == b'\n')
                .map(|pos| offset - pos)
                .unwrap_or(0);

            // Find the full line after that newline
            let line = input[line_begin..]
                .lines()
                .next()
                .unwrap_or(&input[line_begin..])
                .trim_end();

            // The (1-indexed) column number is the offset of our substring into that line
            let column_number = line.offset(substring) + 1;

            match kind {
                VerboseErrorKind::Char(c) => {
                    if let Some(actual) = substring.chars().next() {
                        write!(
                            &mut result,
                            "{i}: at line {line_number}:\n\
               {line}\n\
               {caret:>column$}\n\
               expected '{expected}', found {actual}\n\n",
                            i = i,
                            line_number = line_number,
                            line = line,
                            caret = '^',
                            column = column_number,
                            expected = c,
                            actual = actual,
                        )
                    } else {
                        write!(
                            &mut result,
                            "{i}: at line {line_number}:\n\
               {line}\n\
               {caret:>column$}\n\
               expected '{expected}', got end of input\n\n",
                            i = i,
                            line_number = line_number,
                            line = line,
                            caret = '^',
                            column = column_number,
                            expected = c,
                        )
                    }
                }
                VerboseErrorKind::Context(s) => write!(
                    &mut result,
                    "{i}: at line {line_number}, in {context}:\n\
             {line}\n\
             {caret:>column$}\n\n",
                    i = i,
                    line_number = line_number,
                    context = s,
                    line = line,
                    caret = '^',
                    column = column_number,
                ),
                VerboseErrorKind::Nom(e) => write!(
                    &mut result,
                    "{i}: at line {line_number}, in {nom_err:?}:\n\
             {line}\n\
             {caret:>column$}\n\n",
                    i = i,
                    line_number = line_number,
                    nom_err = e,
                    line = line,
                    caret = '^',
                    column = column_number,
                ),
            }
        }
        // Because `write!` to a `String` is infallible, this `unwrap` is fine.
        .unwrap();
    }

    result
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

pub fn format_error(filepath: &str, file_contents: &str, error: typecheck::Error) -> String {
    let path_length = filepath.len();
    let separator = "-".repeat(80 - path_length);

    let highlight = prepare_highlight(file_contents, &error);
    let description = to_error_description(&error);

    format!(
        "{}{}\n\n{}\n    {}",
        filepath.cyan(),
        separator.cyan(),
        highlight,
        description
    )
}

fn prepare_highlight(file_contents: &str, error: &typecheck::Error) -> String {
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

fn render_highlight_location(
    file_contents: &str,
    rendered: &mut String,
    location: &typecheck::Location,
) {
    match &location.primary {
        None => (),
        Some(primary) => {
            let mut last_line_index: usize = 0;
            let mut first_rendered = false;
            for context in &location.contexts {
                rendered.push_str(&get_line(&file_contents, false, context.start.line));
                rendered.push_str("\n");
                if first_rendered && context.start.line.to_usize() > last_line_index + 1 {
                    rendered.push_str(&"    |        ...\n".truecolor(120, 120, 120).to_string())
                }

                first_rendered = true;
                last_line_index = context.start.line.to_usize();
            }

            rendered.push_str(&get_line(file_contents, true, primary.start.line));
            rendered.push_str("\n");
            rendered.push_str(&highlight_line(&primary));
            rendered.push_str("\n");

            last_line_index = primary.start.line.to_usize();

            for light in &location.contexts {
                if light.start.line.to_usize() > last_line_index + 1 {
                    rendered.push_str(&"    |        ...\n".truecolor(120, 120, 120).to_string())
                }

                rendered.push_str(&get_line(&file_contents, false, light.end.line));
                rendered.push_str("\n");
            }
        }
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

fn highlight_line(range: &typecheck::Range) -> String {
    if range.start.column < range.end.column && range.start.line == range.end.line {
        let indent = " ".repeat(range.start.column);
        let highlight = "^".repeat(range.end.column - range.start.column);
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

fn to_error_description(error: &typecheck::Error) -> String {
    match &error.error_type {
        typecheck::ErrorType::DuplicateDefinition(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are two definitions for {}\n",
                name.yellow()
            ));

            result.push_str("\n\n");
            result
        }

        typecheck::ErrorType::DefinitionIsBuiltIn(name) => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "The {} type is a built-in type, try using another name.\n",
                name.yellow()
            ));

            result.push_str("\n\n");
            result
        }
        typecheck::ErrorType::DuplicateField { record, field } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "There are multiple definitions for {} on {}.\n",
                field.yellow(),
                record.cyan()
            ));

            result.push_str("\n\n");
            result
        }
        typecheck::ErrorType::UnknownTable { found, existing } => {
            let mut result = "".to_string();
            result.push_str(&format!(
                "I don't recognize the '{}' table, is that a typo?\n",
                found.yellow()
            ));

            if existing.len() > 0 {
                result.push_str("\nThese tables might be similar\n");
                for table in existing {
                    result.push_str(&format!("    {}", table.cyan()));
                }
            }

            result.push_str("\n\n");
            result
        }
        typecheck::ErrorType::UnusedParam { param } => {
            let mut result = "".to_string();
            let colored_param = format!("${}", param).yellow();

            result.push_str(&format!(
                "{} isn't being used. Let's either use it or remove it.",
                colored_param
            ));

            result.push_str("\n\n");
            result
        }
        typecheck::ErrorType::UnknownType(found) => {
            let mut result = "".to_string();
            let colored_param = format!("{}", found).cyan();

            result.push_str(&format!(
                "I don't recognize the {} type, is that a typo?",
                colored_param
            ));

            result.push_str("\n\n");
            result
        }

        typecheck::ErrorType::UnknownField {
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
                "\n\nHere are the fields on {}:\n\n",
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
                    "    {}:{}{}\n",
                    field_name.yellow(),
                    spacing,
                    field_type.cyan()
                ));
            }

            result.push_str("\n\n");
            result
        }
        _ => format!("{:#?}\n", error.error_type),
    }
}
