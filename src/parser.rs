use crate::ast;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while},
    character::complete::{
        alpha1, alphanumeric1, char, digit1, multispace0, multispace1, newline, one_of,
    },
    combinator::{map_res, opt, recognize},
    multi::{many0, many1, separated_list0, separated_list1},
    sequence::{delimited, tuple},
    IResult,
};

pub fn run(input: &str) -> Result<ast::Schema, String> {
    match parse_schema(input) {
        Ok((remaining, schema)) => {
            if !remaining.is_empty() {
                return Err(format!("Error: Unparsed input: {:?}", remaining));
            } else {
                return Ok(schema);
            }
        }
        Err(e) => Err(format!("Error: {:?}", e)),
    }
}

fn parse_schema(input: &str) -> IResult<&str, ast::Schema> {
    let (input, _) = multispace0(input)?;
    let (input, definitions) = many0(parse_definition)(input)?;
    let (input, _) = multispace0(input)?;
    Ok((input, ast::Schema { definitions }))
}

fn parse_definition(input: &str) -> IResult<&str, ast::Definition> {
    alt((parse_comment, parse_tagged, parse_record, parse_lines))(input)
}

fn parse_lines(input: &str) -> IResult<&str, ast::Definition> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::Definition::Lines { count }))
}

fn parse_comment(input: &str) -> IResult<&str, ast::Definition> {
    let (input, _) = tag("//")(input)?;
    let (input, text) = take_until("\n")(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::Definition::Comment {
            text: text.to_string(),
        },
    ))
}

fn parse_typename(input: &str) -> IResult<&str, &str> {
    alphanumeric1(input)
}

// A parser to check if a character is lowercase
fn is_lowercase_char(c: char) -> bool {
    c.is_ascii_lowercase()
}

// A parser for a lowercase letter followed by alphanumeric characters
fn parse_fieldname(input: &str) -> IResult<&str, &str> {
    // let (input, start) = recognize(char::is_lowercase)(input)?;
    // let (input, rest) = take_while(|c: char| c.is_alphanumeric())(input)?;
    // Ok((input, &input[start.len()..]))
    let (input, name) = alphanumeric1(input)?;
    Ok((input, name))
}

fn parse_record(input: &str) -> IResult<&str, ast::Definition> {
    let (input, _) = tag("record")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = parse_record_fields(input)?;
    let (input, _) = newline(input)?;

    Ok((
        input,
        ast::Definition::Record {
            name: name.to_string(),
            fields,
        },
    ))
}

fn parse_record_fields(input: &str) -> IResult<&str, Vec<ast::Field>> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("{")(input)?;
    let (input, fields) = many0(parse_field)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("}")(input)?;
    Ok((input, fields))
}

fn parse_field(input: &str) -> IResult<&str, ast::Field> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, type_) = parse_typename(input)?;
    Ok((
        input,
        ast::Field {
            name: name.to_string(),
            type_: type_.to_string(),
            directives: vec![],
        },
    ))
}

fn parse_type_separator(input: &str) -> IResult<&str, char> {
    delimited(multispace0, char('|'), multispace0)(input)
}

fn parse_tagged(input: &str) -> IResult<&str, ast::Definition> {
    let (input, _) = tag("type")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, variants) = separated_list0(parse_type_separator, parse_variant)(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::Definition::Tagged {
            name: name.to_string(),
            variants,
        },
    ))
}

fn parse_variant(input: &str) -> IResult<&str, ast::Variant> {
    let (input, name) = parse_typename(input)?;
    let (input, optionalFields) = opt(parse_record_fields)(input)?;

    Ok((
        input,
        ast::Variant {
            name: name.to_string(),
            data: optionalFields,
        },
    ))
}

// Parse Query
//

pub fn parse_query(input: &str) -> Result<ast::QueryList, String> {
    match parse_query_list(input) {
        Ok((remaining, query_list)) => {
            if !remaining.is_empty() {
                return Err(format!("Error: Unparsed input: {:?}", remaining));
            } else {
                return Ok(query_list);
            }
        }
        Err(e) => Err(format!("Error: {:?}", e)),
    }
}

fn parse_query_list(input: &str) -> IResult<&str, ast::QueryList> {
    let (input, _) = multispace0(input)?;
    let (input, queries) = many0(parse_query_def)(input)?;
    let (input, _) = multispace0(input)?;
    Ok((input, ast::QueryList { queries }))
}

fn parse_query_def(input: &str) -> IResult<&str, ast::QueryDef> {
    alt((parse_query_comment, parse_query_details, parse_query_lines))(input)
}

fn parse_query_comment(input: &str) -> IResult<&str, ast::QueryDef> {
    let (input, _) = tag("//")(input)?;
    let (input, text) = take_until("\n")(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::QueryDef::QueryComment {
            text: text.to_string(),
        },
    ))
}

fn parse_query_lines(input: &str) -> IResult<&str, ast::QueryDef> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::QueryDef::QueryLines { count }))
}

fn parse_query_details(input: &str) -> IResult<&str, ast::QueryDef> {
    let (input, _) = tag("query")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = parse_query_fieldblock(input)?;

    Ok((
        input,
        ast::QueryDef::Query(ast::Query {
            name: name.to_string(),
            args: vec![],
            fields,
        }),
    ))
}

fn parse_query_fieldblock(input: &str) -> IResult<&str, Vec<ast::QueryField>> {
    let (input, _) = tag("{")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = many0(parse_query_field)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("}")(input)?;
    Ok((input, fields))
}

fn parse_query_field(input: &str) -> IResult<&str, ast::QueryField> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fieldsOrNone) = opt(parse_query_fieldblock)(input)?;

    Ok((
        input,
        ast::QueryField {
            name: name.to_string(),
            args: vec![],
            directives: vec![],
            fields: fieldsOrNone.unwrap_or_else(Vec::new),
        },
    ))
}
