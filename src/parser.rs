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
    let (input, unsorted_fields) = parse_record_fields(input)?;
    let (input, _) = newline(input)?;

    let mut fields = unsorted_fields.clone();

    fields.sort_by(ast::column_order);
    insert_after_last_instance(
        &mut fields,
        ast::is_field_directive,
        ast::Field::ColumnLines { count: 1 },
    );

    Ok((
        input,
        ast::Definition::Record {
            name: name.to_string(),
            fields,
        },
    ))
}

fn insert_after_first_instance<T, F>(vec: &mut Vec<T>, predicate: F, value: T)
where
    F: Fn(&T) -> bool,
{
    if let Some(pos) = vec.iter().position(predicate) {
        vec.insert(pos + 1, value);
    }
}

fn insert_after_last_instance<T, F>(vec: &mut Vec<T>, predicate: F, value: T)
where
    F: Fn(&T) -> bool,
{
    if let Some(pos) = vec.iter().rev().position(predicate) {
        vec.insert(vec.len() - pos, value);
    }
}

fn insert_after_first_instance_continuous<T, F, IfPrevious>(
    vec: &mut Vec<T>,
    predicate: F,
    insert_if_previous: IfPrevious,
    value: T,
) where
    F: Fn(&T) -> bool,
    IfPrevious: Fn(Option<&T>) -> bool,
    T: Clone,
{
    let mut in_sequence = false;
    let mut indices: Vec<usize> = vec![];
    let mut target_index = 0;
    let mut previous_item: Option<&T> = None;

    for (i, item) in vec.iter().enumerate() {
        if predicate(item) {
            if !in_sequence & insert_if_previous(previous_item) {
                in_sequence = true;
                indices.push(target_index);
                target_index = target_index + 1;
            }
        } else {
            in_sequence = false;
        }
        previous_item = Some(item);
        target_index = target_index + 1;
    }

    for index in indices {
        vec.insert(index, value.clone());
    }
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
    alt((
        parse_field_comment,
        parse_column_field,
        parse_field_directive,
    ))(input)
}

fn parse_field_comment(input: &str) -> IResult<&str, ast::Field> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("//")(input)?;
    let (input, text) = take_until("\n")(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::Field::ColumnComment {
            text: text.to_string(),
        },
    ))
}

fn parse_field_directive(input: &str) -> IResult<&str, ast::Field> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@")(input)?;
    let (input, name) = parse_typename(input)?;
    match name {
        "tablename" => {
            let (input, _) = multispace1(input)?;
            let (input, tablename) = parse_string_literal(input)?;
            let (input, _) = multispace0(input)?;

            let directive = ast::FieldDirective::TableName(tablename.to_string());
            return Ok((input, ast::Field::FieldDirective(directive)));
        }
        "link" => {
            let (input, _) = multispace1(input)?;
            let (input, linkname) = parse_fieldname(input)?;

            // link details
            // { from: userId, to: User.id }

            let (input, _) = multispace1(input)?;
            let (input, _) = tag("{")(input)?;
            let (input, fields) = many0(parse_link_field)(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = tag("}")(input)?;
            let (input, _) = multispace0(input)?;

            // gather into link details
            let (input, link_details) = link_field_to_details(input, linkname, fields)?;

            let (input, _) = multispace0(input)?;

            return Ok((
                input,
                ast::Field::FieldDirective(ast::FieldDirective::Link(link_details)),
            ));
        }
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum LinkField {
    From(Vec<String>),
    To { table: String, id: Vec<String> },
}

struct ToDetails {
    table: String,
    id: Vec<String>,
}

fn link_field_to_details<'a, 'b>(
    input: &'a str,
    linkname: &'b str,
    link_fields: Vec<LinkField>,
) -> IResult<&'a str, ast::LinkDetails> {
    let mut details = ast::LinkDetails {
        link_name: linkname.to_string(),
        local_ids: vec![],
        foreign_tablename: "".to_string(),
        foreign_ids: vec![],
    };
    let mut has_from = false;
    let mut has_to = false;
    for link in link_fields {
        match link {
            LinkField::From(idList) => {
                if has_from {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Tag,
                    )));
                } else {
                    has_from = true;
                    details.local_ids = idList
                }
            }
            LinkField::To { table, id } => {
                if has_to {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Tag,
                    )));
                } else {
                    has_to = true;
                    details.foreign_tablename = table;
                    details.foreign_ids = id;
                }
            }
        }
    }
    if (has_to & has_from) {
        return Ok((input, details));
    }

    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

fn parse_link_field(input: &str) -> IResult<&str, LinkField> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;

    match name {
        "to" => {
            let (input, to_table) = parse_typename(input)?;
            let (input, _) = tag(".")(input)?;
            let (input, to_id) = parse_fieldname(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = opt(tag(","))(input)?;
            let (input, _) = multispace0(input)?;
            return Ok((
                input,
                LinkField::To {
                    table: to_table.to_string(),
                    id: vec![to_id.to_string()],
                },
            ));
        }
        "from" => {
            let (input, from_field) = parse_fieldname(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = opt(tag(","))(input)?;
            let (input, _) = multispace0(input)?;
            return Ok((input, LinkField::From(vec![from_field.to_string()])));
        }
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    }
}

fn parse_column_field(input: &str) -> IResult<&str, ast::Field> {
    let (input, column) = parse_column(input)?;
    Ok((input, ast::Field::Column(column)))
}

fn parse_column(input: &str) -> IResult<&str, ast::Column> {
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
        ast::Column {
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
    let (input, op) = alt((
        parse_token("query", ast::QueryOperation::Select),
        parse_token("insert", ast::QueryOperation::Insert),
        parse_token("update", ast::QueryOperation::Update),
        parse_token("delete", ast::QueryOperation::Delete),
    ))(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, paramDefinitionsOrNone) = opt(parse_query_param_definitions)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = parse_query_fieldblock(input)?;

    Ok((
        input,
        ast::QueryDef::Query(ast::Query {
            operation: op,
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

fn parse_query_arg_fieldblock(input: &str) -> IResult<&str, Vec<ast::ArgField>> {
    let (input, _) = tag("{")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, mut fields) = many0(parse_arg_field)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("}")(input)?;

    fields.sort_by(ast::query_field_order);

    insert_after_last_instance(
        &mut fields,
        ast::is_query_field_arg,
        ast::ArgField::Line { count: 1 },
    );

    Ok((input, fields))
}

fn parse_alias(input: &str) -> IResult<&str, String> {
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, alias) = parse_fieldname(input)?;
    Ok((input, alias.to_string()))
}

fn parse_arg_field(input: &str) -> IResult<&str, ast::ArgField> {
    alt((parse_query_arg_field, parse_arg))(input)
}

fn parse_arg(input: &str) -> IResult<&str, ast::ArgField> {
    let (input, arg) = parse_query_arg(input)?;
    Ok((input, ast::ArgField::Arg(arg)))
}

fn parse_query_arg_field(input: &str) -> IResult<&str, ast::ArgField> {
    let (input, q) = parse_query_field(input)?;
    Ok((input, ast::ArgField::Field(q)))
}

fn parse_query_field(input: &str) -> IResult<&str, ast::QueryField> {
    let (input, _) = multispace0(input)?;
    let (input, name_or_alias) = parse_fieldname(input)?;
    let (input, alias_or_name) = opt(parse_alias)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fieldsOrNone) = opt(parse_query_arg_fieldblock)(input)?;

    let (name, alias) = match alias_or_name {
        Some(alias) => (alias, Some(name_or_alias.to_string())),
        None => (name_or_alias.to_string(), None),
    };

    Ok((
        input,
        ast::QueryField {
            name: name.to_string(),
            alias,
            directives: vec![],
            fields: fieldsOrNone.unwrap_or_else(Vec::new),
        },
    ))
}

fn parse_query_arg(input: &str) -> IResult<&str, ast::Arg> {
    alt((parse_limit, parse_offset, parse_sort, parse_where))(input)
}

fn parse_limit(input: &str) -> IResult<&str, ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@limit")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;

    Ok((input, ast::Arg::Limit(val)))
}

fn parse_offset(input: &str) -> IResult<&str, ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@offset")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, ast::Arg::Offset(val)))
}

fn parse_sort(input: &str) -> IResult<&str, ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@sort")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, field) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, direction) = alt((
        parse_token("asc", ast::Direction::Asc),
        parse_token("desc", ast::Direction::Desc),
    ))(input)?;

    Ok((input, ast::Arg::OrderBy(direction, field.to_string())))
}

#[derive(Debug, Clone)]
enum AndOr {
    And,
    Or,
}

fn parse_and_or(input: &str) -> IResult<&str, AndOr> {
    alt((parse_token("&&", AndOr::And), parse_token("||", AndOr::Or)))(input)
}

fn parse_where(input: &str) -> IResult<&str, ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@where")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, where_arg) = parse_where_arg(input)?;
    Ok((input, ast::Arg::Where(where_arg)))
}

fn parse_where_arg(input: &str) -> IResult<&str, ast::WhereArg> {
    let (input, _) = multispace0(input)?;
    let (input, where_col) = parse_query_where(input)?;
    let (input, _) = multispace0(input)?;
    let (input, maybe_and_or) = opt(parse_and_or)(input)?;
    match maybe_and_or {
        Some(AndOr::And) => {
            let (input, where_col2) = parse_query_where(input)?;
            Ok((input, ast::WhereArg::And(vec![where_col, where_col2])))
        }
        Some(AndOr::Or) => {
            let (input, where_col2) = parse_query_where(input)?;
            Ok((input, ast::WhereArg::Or(vec![where_col, where_col2])))
        }
        None => Ok((input, where_col)),
    }
}

fn parse_query_where(input: &str) -> IResult<&str, ast::WhereArg> {
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, _) = multispace0(input)?;
    let (input, operator) = parse_operator(input)?;
    let (input, _) = multispace0(input)?;
    let (input, value) = parse_value(input)?;

    Ok((
        input,
        ast::WhereArg::Column(name.to_string(), operator, value),
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

fn parse_int(input: &str) -> IResult<&str, usize> {
    let (input, value) = digit1(input)?;
    Ok((input, value.parse().unwrap()))
}

fn parse_string(input: &str) -> IResult<&str, ast::QueryValue> {
    let (input, value) = parse_string_literal(input)?;
    Ok((input, ast::QueryValue::String(value.to_string())))
}

fn parse_string_literal(input: &str) -> IResult<&str, &str> {
    let (input, _) = tag("\"")(input)?;
    let (input, value) = take_until("\"")(input)?;
    let (input, _) = tag("\"")(input)?;
    Ok((input, value))
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
