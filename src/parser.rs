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
    let (input, is_nullable) = parse_nullable(input)?;
    let (input, _) = multispace0(input)?;
    let (input, directives) = many0(parse_column_directive)(input)?;

    Ok((
        input,
        ast::Field {
            name: name.to_string(),
            type_: type_.to_string(),
            nullable: is_nullable,
            serialization_type: to_serialization_type(type_),
            directives,
        },
    ))
}

fn parse_nullable(input: &str) -> IResult<&str, bool> {
    let (input, maybeNullable) = opt(char('?'))(input)?;
    Ok((input, maybeNullable != None))
}

fn parse_column_directive(input: &str) -> IResult<&str, ast::ColumnDirective> {
    alt((
        parse_directive_named("id", ast::ColumnDirective::PrimaryKey),
        parse_directive_named("unique", ast::ColumnDirective::Unique),
    ))(input)
}

fn parse_directive_named<'a, T>(
    tag_str: &'a str,
    value: T,
) -> impl Fn(&'a str) -> IResult<&'a str, T> + 'a
where
    T: Clone + 'a,
{
    move |input: &'a str| {
        let (input, _) = tag("@")(input)?;
        let (input, _) = tag(tag_str)(input)?;
        Ok((input, value.clone()))
    }
}

fn to_serialization_type(type_: &str) -> ast::SerializationType {
    match type_ {
        "String" => ast::SerializationType::Text,
        "Int" => ast::SerializationType::Integer,
        "Float" => ast::SerializationType::Real,
        "Bool" => ast::SerializationType::Integer,
        _ => ast::SerializationType::BlobWithSchema(type_.to_string()),
    }
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
    let (input, paramDefinitionsOrNone) = opt(parse_query_param_definitions)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = parse_query_fieldblock(input)?;

    Ok((
        input,
        ast::QueryDef::Query(ast::Query {
            name: name.to_string(),
            args: paramDefinitionsOrNone.unwrap_or(vec![]),
            fields,
        }),
    ))
}

fn parse_query_param_definitions(input: &str) -> IResult<&str, Vec<ast::QueryParamDefinition>> {
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = many0(parse_query_param_definition)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, fields))
}

fn parse_query_param_definition(input: &str) -> IResult<&str, ast::QueryParamDefinition> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("$")(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, typename) = parse_typename(input)?;

    Ok((
        input,
        ast::QueryParamDefinition {
            name: name.to_string(),
            type_: typename.to_string(),
        },
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
    let (input, paramsOrNone) = opt(parse_query_paramblock)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fieldsOrNone) = opt(parse_query_fieldblock)(input)?;

    Ok((
        input,
        ast::QueryField {
            name: name.to_string(),
            params: paramsOrNone.unwrap_or_else(Vec::new),
            directives: vec![],
            fields: fieldsOrNone.unwrap_or_else(Vec::new),
        },
    ))
}

fn parse_query_paramblock(input: &str) -> IResult<&str, Vec<ast::QueryParam>> {
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = many0(parse_query_param)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, fields))
}

fn parse_query_param(input: &str) -> IResult<&str, ast::QueryParam> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, operator) = parse_operator(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_value(input)?;

    Ok((
        input,
        ast::QueryParam {
            name: name.to_string(),
            operator: operator,
            value: value,
        },
    ))
}

fn parse_operator(input: &str) -> IResult<&str, ast::Operator> {
    alt((
        parse_token("=", ast::Operator::Equal),
        parse_token("<", ast::Operator::LessThan),
        parse_token(">", ast::Operator::GreaterThan),
        parse_token("<=", ast::Operator::LessThanOrEqual),
        parse_token(">=", ast::Operator::GreaterThanOrEqual),
        parse_token("in", ast::Operator::In),
    ))(input)
}

fn parse_variable(input: &str) -> IResult<&str, ast::QueryValue> {
    let (input, _) = tag("$")(input)?;
    let (input, name) = parse_fieldname(input)?;
    Ok((input, ast::QueryValue::Variable(name.to_string())))
}

fn parse_value(input: &str) -> IResult<&str, ast::QueryValue> {
    alt((
        parse_token("null", ast::QueryValue::Null),
        parse_variable,
        parse_string,
        parse_number,
        // tag("true"),
        // tag("false"),
        // parse_number,
        // parse_string,
        // parse_array,
        // parse_object,
    ))(input)
}

//
// Parses the first digits, starting with any character except 0
// If there is a ., then it must be a `ast::Float`, otherwise it's an ast::Int.
fn parse_number(input: &str) -> IResult<&str, ast::QueryValue> {
    let (input, first) = one_of("123456789")(input)?;
    let (input, rest) = take_while(|c: char| c.is_digit(10))(input)?;
    let (input, dot) = opt(tag("."))(input)?;
    let (input, tail) = take_while(|c: char| c.is_digit(10))(input)?;

    let period = dot.unwrap_or("");

    let value = format!("{}{}{}{}", first, rest, period, tail);
    if value.contains(".") {
        Ok((input, ast::QueryValue::Float(value.parse().unwrap())))
    } else {
        Ok((input, ast::QueryValue::Int(value.parse().unwrap())))
    }
}

fn parse_string(input: &str) -> IResult<&str, ast::QueryValue> {
    let (input, _) = tag("\"")(input)?;
    let (input, value) = take_until("\"")(input)?;
    let (input, _) = tag("\"")(input)?;
    Ok((input, ast::QueryValue::String(value.to_string())))
}

fn parse_token<'a, T>(tag_str: &'a str, value: T) -> impl Fn(&'a str) -> IResult<&'a str, T> + 'a
where
    T: Clone + 'a,
{
    move |input: &'a str| {
        let (input, _) = tag(tag_str)(input)?;
        Ok((input, value.clone()))
    }
}
