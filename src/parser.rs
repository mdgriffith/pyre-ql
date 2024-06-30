use crate::ast;
use crate::error;
use nom::bytes::complete::take_while1;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while},
    character::complete::{
        alpha1, alphanumeric1, char, digit1, multispace0, multispace1, newline, one_of,
    },
    combinator::{cut, eof, map_res, opt, recognize},
    error::{convert_error, Error, VerboseError, VerboseErrorKind},
    multi::{many0, many1, separated_list0, separated_list1},
    sequence::{delimited, tuple},
    IResult,
};
use nom_locate::{position, LocatedSpan};

pub type Text<'a> = LocatedSpan<&'a str>;

type ParseResult<'a, Output> = IResult<Text<'a>, Output, VerboseError<Text<'a>>>;

pub fn run(input_string: &str) -> Result<ast::Schema, String> {
    let input = Text::new(input_string);

    match parse_schema(input) {
        Ok((remaining, schema)) => {
            return Ok(schema);
        }
        Err(e) => {
            return Err(render_error(input, e));
        }
    }
}

fn render_error(
    input: LocatedSpan<&str>,
    err: nom::Err<VerboseError<LocatedSpan<&str>>>,
) -> String {
    match err {
        nom::Err::Incomplete(_) => {
            return "Incomplete".to_string();
        }
        nom::Err::Error(error) => {
            let err_text: String = error::convert_error(input, error);
            println!("PARSER ERROR");
            return err_text;
        }
        nom::Err::Failure(e) => {
            // return Err(convert_error(Text::new(input), e));
            return "Failure".to_string();
        }
    }
}

fn parse_schema(input: Text) -> ParseResult<ast::Schema> {
    let (input, _) = multispace0(input)?;
    let (input, definitions) = many1(parse_definition)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = eof(input)?;
    Ok((input, ast::Schema { definitions }))
}

fn parse_definition(input: Text) -> ParseResult<ast::Definition> {
    alt((parse_comment, parse_tagged, parse_record, parse_lines))(input)
}

fn parse_lines(input: Text) -> ParseResult<ast::Definition> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::Definition::Lines { count }))
}

fn parse_comment(input: Text) -> ParseResult<ast::Definition> {
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

fn parse_typename(input: Text) -> ParseResult<&str> {
    let (input, val) = alphanumeric1(input)?;
    Ok((input, val.fragment()))
}

// A parser to check if a character is lowercase
fn is_lowercase_char(c: char) -> bool {
    c.is_ascii_lowercase()
}

fn parse_fieldname(input: Text) -> ParseResult<&str> {
    let (input, val) = recognize(tuple((
        alt((
            take_while1(|c: char| c.is_lowercase()), // First character can be lowercase
            take_while1(|c: char| c == '_'),         // or an underscore
        )),
        many0(alt((alphanumeric1, take_while1(|c: char| c == '_')))), // Followed by alphanumeric or underscores
    )))(input)?;

    Ok((input, val.fragment()))
}

fn parse_record(input: Text) -> ParseResult<ast::Definition> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("record")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, start_name_pos) = position(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = with_braces(parse_field)(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = newline(input)?;

    Ok((
        input,
        ast::Definition::Record {
            name: name.to_string(),
            fields,
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),
            start_name: Some(to_location(start_name_pos)),
            end_name: Some(to_location(end_name_pos)),
        },
    ))
}

fn parse_field(input: Text) -> ParseResult<ast::Field> {
    alt((
        parse_field_comment,
        parse_column_field,
        parse_field_directive,
    ))(input)
}

fn parse_field_comment(input: Text) -> ParseResult<ast::Field> {
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

fn parse_field_directive(input: Text) -> ParseResult<ast::Field> {
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
            let (input, fields) = with_braces(parse_link_field)(input)?;
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
            return Err(nom::Err::Error(VerboseError {
                errors: vec![(input, VerboseErrorKind::Context("Unknown directive"))],
            }));
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
    input: Text<'a>,
    linkname: &'b str,
    link_fields: Vec<LinkField>,
) -> ParseResult<'a, ast::LinkDetails> {
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
                    return Err(nom::Err::Error(VerboseError {
                        errors: vec![(input, VerboseErrorKind::Context("tag"))],
                    }));
                } else {
                    has_from = true;
                    details.local_ids = idList
                }
            }
            LinkField::To { table, id } => {
                if has_to {
                    return Err(nom::Err::Error(VerboseError {
                        errors: vec![(input, VerboseErrorKind::Context("tag"))],
                    }));
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

    Err(nom::Err::Error(VerboseError {
        errors: vec![(input, VerboseErrorKind::Context("tag"))],
    }))
}

fn parse_link_field(input: Text) -> ParseResult<LinkField> {
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
            // return Err(nom::Err::Error(nom::error::Error::new(
            //     input,
            //     nom::error::ErrorKind::Tag,
            // )))
            return Err(nom::Err::Error(VerboseError {
                errors: vec![(input, VerboseErrorKind::Context("tag"))],
            }));
        }
    }
}

fn parse_column_field(input: Text) -> ParseResult<ast::Field> {
    let (input, column) = parse_column(input)?;
    Ok((input, ast::Field::Column(column)))
}

fn to_location(pos: LocatedSpan<&str>) -> ast::Location {
    ast::Location {
        offset: pos.location_offset(),
        line: pos.location_line(),
        column: pos.get_column(),
    }
}

fn parse_column(input: Text) -> ParseResult<ast::Column> {
    let (input, _) = multispace0(input)?;
    let (input, start_pos) = position(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, type_) = parse_typename(input)?;
    let (input, is_nullable) = parse_nullable(input)?;
    let (input, _) = multispace0(input)?;
    let (input, directives) = many0(parse_column_directive)(input)?;

    let (input, end_pos) = position(input)?;

    Ok((
        input,
        ast::Column {
            name: name.to_string(),
            type_: type_.to_string(),
            nullable: is_nullable,
            serialization_type: to_serialization_type(type_),
            directives,
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),

            start_name: Some(to_location(start_pos)),
            end_name: Some(to_location(end_name_pos)),
        },
    ))
}

fn parse_nullable(input: Text) -> ParseResult<bool> {
    let (input, maybeNullable) = opt(char('?'))(input)?;
    Ok((input, maybeNullable != None))
}

fn parse_column_directive(input: Text) -> ParseResult<ast::ColumnDirective> {
    alt((
        parse_directive_named("id", ast::ColumnDirective::PrimaryKey),
        parse_directive_named("unique", ast::ColumnDirective::Unique),
        parse_default_directive,
    ))(input)
}

fn parse_default_directive(input: Text) -> ParseResult<ast::ColumnDirective> {
    let (input, _) = tag("@default(")(input)?;

    let (input, _) = multispace0(input)?;
    let (input, default) = alt((
        parse_token("now", ast::DefaultValue::Now),
        parse_default_value,
    ))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, ast::ColumnDirective::Default(default)))
}

fn parse_default_value(input: Text) -> ParseResult<ast::DefaultValue> {
    let (input, val) = parse_value(input)?;
    Ok((input, ast::DefaultValue::Value(val)))
}

fn parse_directive_named<'a, T>(tag_str: &'a str, value: T) -> impl Fn(Text) -> ParseResult<T> + 'a
where
    T: Clone + 'a,
{
    move |input: Text| {
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
        "DateTime" => ast::SerializationType::Integer,
        "Date" => ast::SerializationType::Text,
        _ => ast::SerializationType::BlobWithSchema(type_.to_string()),
    }
}

fn parse_type_separator(input: Text) -> ParseResult<char> {
    delimited(multispace0, char('|'), multispace0)(input)
}

fn parse_tagged(input: Text) -> ParseResult<ast::Definition> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("type")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, variants) = separated_list0(parse_type_separator, parse_variant)(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = newline(input)?;

    Ok((
        input,
        ast::Definition::Tagged {
            name: name.to_string(),
            variants,
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),
        },
    ))
}

fn parse_variant(input: Text) -> ParseResult<ast::Variant> {
    let (input, start_pos) = position(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, _) = multispace0(input)?;
    let (input, optionalFields) = opt(with_braces(parse_field))(input)?;
    let (input, end_pos) = position(input)?;

    Ok((
        input,
        ast::Variant {
            name: name.to_string(),
            data: optionalFields,
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),
        },
    ))
}

// Parse Query
//

pub fn parse_query(input_str: &str) -> Result<ast::QueryList, String> {
    let input = Text::new(input_str);
    match parse_query_list(input) {
        Ok((remaining, query_list)) => {
            return Ok(query_list);
        }
        Err(e) => Err(render_error(input, e)),
    }
}

fn parse_query_list(input: Text) -> ParseResult<ast::QueryList> {
    let (input, _) = multispace0(input)?;
    let (input, queries) = many1(parse_query_def)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = eof(input)?;
    Ok((input, ast::QueryList { queries }))
}

fn parse_query_def(input: Text) -> ParseResult<ast::QueryDef> {
    alt((parse_query_comment, parse_query_details, parse_query_lines))(input)
}

fn parse_query_comment(input: Text) -> ParseResult<ast::QueryDef> {
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

fn parse_query_lines(input: Text) -> ParseResult<ast::QueryDef> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::QueryDef::QueryLines { count }))
}

fn parse_query_details(input: Text) -> ParseResult<ast::QueryDef> {
    let (input, op) = alt((
        parse_token("query", ast::QueryOperation::Select),
        parse_token("insert", ast::QueryOperation::Insert),
        parse_token("update", ast::QueryOperation::Update),
        parse_token("delete", ast::QueryOperation::Delete),
    ))(input)?;
    let (input, _) = multispace1(input)?;
    let (input, start_pos) = position(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, paramDefinitionsOrNone) = opt(parse_query_param_definitions)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) = with_braces(parse_query_field)(input)?;
    let (input, end_pos) = position(input)?;

    Ok((
        input,
        ast::QueryDef::Query(ast::Query {
            operation: op,
            name: name.to_string(),
            args: paramDefinitionsOrNone.unwrap_or(vec![]),
            fields,
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),
        }),
    ))
}

fn parse_query_param_definitions(input: Text) -> ParseResult<Vec<ast::QueryParamDefinition>> {
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fields) =
        separated_list0(parse_param_separator, parse_query_param_definition)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, fields))
}

fn parse_param_separator(input: Text) -> ParseResult<char> {
    delimited(multispace0, char(','), multispace0)(input)
}

fn parse_query_param_definition(input: Text) -> ParseResult<ast::QueryParamDefinition> {
    let (input, _) = multispace0(input)?;
    let (input, start_name_pos) = position(input)?;
    let (input, _) = tag("$")(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, start_type_pos) = position(input)?;
    let (input, typename) = parse_typename(input)?;
    let (input, end_type_pos) = position(input)?;

    Ok((
        input,
        ast::QueryParamDefinition {
            name: name.to_string(),
            type_: typename.to_string(),
            start_name: Some(to_location(start_name_pos)),
            end_name: Some(to_location(end_name_pos)),
            start_type: Some(to_location(start_type_pos)),
            end_type: Some(to_location(end_type_pos)),
        },
    ))
}

fn with_braces<'a, F, T>(parse_term: F) -> impl Fn(Text) -> ParseResult<Vec<T>>
where
    F: Fn(Text) -> ParseResult<T>,
{
    move |input: Text| {
        let (input, _) = tag("{")(input)?;
        let (input, _) = multispace0(input)?;
        let (input, terms) = many0(&parse_term)(input)?;
        let (input, _) = multispace0(input)?;
        let (input, _) = tag("}")(input)?;
        Ok((input, terms))
    }
}

fn parse_set(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, val))
}

fn parse_alias(input: Text) -> ParseResult<String> {
    let (input, _) = tag(":")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, alias) = parse_fieldname(input)?;
    Ok((input, alias.to_string()))
}

fn parse_arg_field(input: Text) -> ParseResult<ast::ArgField> {
    alt((parse_query_arg_field, parse_arg))(input)
}

fn parse_arg(input: Text) -> ParseResult<ast::ArgField> {
    let (input, arg) = parse_query_arg(input)?;
    Ok((input, ast::ArgField::Arg(arg)))
}

fn parse_query_arg_field(input: Text) -> ParseResult<ast::ArgField> {
    let (input, q) = parse_query_field(input)?;
    Ok((input, ast::ArgField::Field(q)))
}

fn parse_query_field(input: Text) -> ParseResult<ast::QueryField> {
    let (input, _) = multispace0(input)?;
    let (input, start_pos) = position(input)?;
    let (input, name_or_alias) = parse_fieldname(input)?;
    let (input, alias_or_name) = opt(parse_alias)(input)?;
    let (input, end_fieldname_pos) = position(input)?;
    let (input, _) = multispace0(input)?;
    let (input, set) = opt(parse_set)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, fieldsOrNone) = opt(with_braces(parse_arg_field))(input)?;
    let (input, end_pos) = position(input)?;

    let (name, alias) = match alias_or_name {
        Some(alias) => (alias, Some(name_or_alias.to_string())),
        None => (name_or_alias.to_string(), None),
    };

    Ok((
        input,
        ast::QueryField {
            name: name.to_string(),
            alias,
            set,
            directives: vec![],
            fields: fieldsOrNone.unwrap_or_else(Vec::new),
            start: Some(to_location(start_pos)),
            end: Some(to_location(end_pos)),

            start_fieldname: Some(to_location(start_pos)),
            end_fieldname: Some(to_location(end_fieldname_pos)),
        },
    ))
}

fn parse_query_arg(input: Text) -> ParseResult<ast::Arg> {
    alt((parse_limit, parse_offset, parse_sort, parse_where))(input)
}

fn parse_limit(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@limit")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;

    Ok((input, ast::Arg::Limit(val)))
}

fn parse_offset(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@offset")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, ast::Arg::Offset(val)))
}

fn parse_sort(input: Text) -> ParseResult<ast::Arg> {
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

fn parse_and_or(input: Text) -> ParseResult<AndOr> {
    alt((parse_token("&&", AndOr::And), parse_token("||", AndOr::Or)))(input)
}

fn parse_where(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("@where")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, where_arg) = parse_where_arg(input)?;
    Ok((input, ast::Arg::Where(where_arg)))
}

fn parse_where_arg(input: Text) -> ParseResult<ast::WhereArg> {
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

fn parse_query_where(input: Text) -> ParseResult<ast::WhereArg> {
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

fn parse_operator(input: Text) -> ParseResult<ast::Operator> {
    alt((
        parse_token("=", ast::Operator::Equal),
        parse_token("<", ast::Operator::LessThan),
        parse_token(">", ast::Operator::GreaterThan),
        parse_token("<=", ast::Operator::LessThanOrEqual),
        parse_token(">=", ast::Operator::GreaterThanOrEqual),
        parse_token("in", ast::Operator::In),
    ))(input)
}

fn parse_variable(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, _) = tag("$")(input)?;
    let (input, name) = parse_fieldname(input)?;
    Ok((input, ast::QueryValue::Variable(name.to_string())))
}

fn parse_value(input: Text) -> ParseResult<ast::QueryValue> {
    alt((
        parse_token("null", ast::QueryValue::Null),
        parse_token("Null", ast::QueryValue::Null),
        parse_token("True", ast::QueryValue::Bool(true)),
        parse_token("False", ast::QueryValue::Bool(false)),
        parse_token("true", ast::QueryValue::Bool(true)),
        parse_token("false", ast::QueryValue::Bool(false)),
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
pub fn parse_number(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, first) = one_of("1234567890")(input)?;
    if first == '0' {
        Ok((input, ast::QueryValue::Int(0)))
    } else {
        let (input, rest) = take_while(|c: char| c.is_digit(10))(input)?;
        let (input, dot) = opt(tag("."))(input)?;
        let (input, tail) = take_while(|c: char| c.is_digit(10))(input)?;

        let period = if dot.is_some() { "." } else { "" };

        let value = format!("{}{}{}{}", first, rest, period, tail);
        if value.contains(".") {
            Ok((input, ast::QueryValue::Float(value.parse().unwrap())))
        } else {
            Ok((input, ast::QueryValue::Int(value.parse().unwrap())))
        }
    }
}

fn parse_int(input: Text) -> ParseResult<usize> {
    let (input, value) = digit1(input)?;
    Ok((input, value.parse().unwrap()))
}

fn parse_string(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, value) = parse_string_literal(input)?;
    Ok((input, ast::QueryValue::String(value.to_string())))
}

fn parse_string_literal(input: Text) -> ParseResult<&str> {
    let (input, _) = tag("\"")(input)?;
    let (input, value) = take_until("\"")(input)?;
    let (input, _) = tag("\"")(input)?;
    Ok((input, value.fragment()))
}

fn parse_token<'a, T>(tag_str: &'a str, value: T) -> impl Fn(Text) -> ParseResult<T> + 'a
where
    T: Clone + 'a,
{
    move |input: Text| {
        let (input, _) = tag(tag_str)(input)?;
        Ok((input, value.clone()))
    }
}
