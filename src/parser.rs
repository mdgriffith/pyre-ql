use crate::ast;
use crate::platform;
use nom::bytes::complete::take_while1;
use nom::character::streaming::{space0, space1};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while},
    character::complete::{
        alphanumeric1, char, line_ending, multispace0, multispace1, newline, one_of,
    },
    combinator::{all_consuming, cut, eof, opt, recognize, value},
    error::{VerboseError, VerboseErrorKind},
    multi::{many0, many1, separated_list0, separated_list1},
    sequence::{delimited, tuple},
    IResult,
};
use nom_locate::{position, LocatedSpan};

pub type Text<'a> = LocatedSpan<&'a str, ParseContext<'a>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseContext<'a> {
    pub file: &'a str,
    pub namespace: String,
    pub expecting: crate::error::Expecting,
}

pub fn placeholder_context<'a>() -> ParseContext<'a> {
    ParseContext {
        file: "placeholder.txt",
        namespace: ast::DEFAULT_SCHEMANAME.to_string(),
        expecting: crate::error::Expecting::PyreFile,
    }
}

fn expecting(input: Text, expecting: crate::error::Expecting) -> Text {
    input.map_extra(|mut ctxt| {
        ctxt.expecting = expecting;
        ctxt
    })
}

type ParseResult<'a, Output> = IResult<Text<'a>, Output, VerboseError<Text<'a>>>;

pub fn run<'a>(
    path: &'a str,
    input_string: &'a str,
    schema: &'a mut ast::Schema,
) -> Result<(), nom::Err<VerboseError<Text<'a>>>> {
    let input = Text::new_extra(
        input_string,
        ParseContext {
            file: path,
            namespace: schema.namespace.clone(),
            expecting: crate::error::Expecting::PyreFile,
        },
    );

    match parse_schema(input, schema) {
        Ok((_, ())) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn render_error(input: &str, err: nom::Err<VerboseError<Text>>) -> String {
    match err {
        nom::Err::Incomplete(_) => {
            return "Incomplete".to_string();
        }
        nom::Err::Error(error) => {
            // println!("PARSER ERROR {:#?}", &error);
            let err_text: String = convert_error(input, error);

            // println!("PARSER ERROR, formatted {:#?}", &err_text);
            // return err_text;
            return err_text;
        }
        nom::Err::Failure(error) => {
            // println!("PARSER ERROR {:#?}", &error);
            let err_text: String = convert_error(input, error);

            // println!("PARSER ERROR, formatted {:#?}", &err_text);
            return err_text;
        }
    }
}

pub fn convert_parsing_error(err: nom::Err<VerboseError<Text>>) -> Option<crate::error::Error> {
    match err {
        nom::Err::Error(error) | nom::Err::Failure(error) => {
            error.errors.get(0).map(|(text, _)| crate::error::Error {
                filepath: text.extra.file.to_string(),
                error_type: crate::error::ErrorType::ParsingError(
                    crate::error::ParsingErrorDetails {
                        expecting: text.extra.expecting.clone(),
                    },
                ),
                locations: vec![crate::error::Location {
                    contexts: vec![],
                    primary: vec![crate::error::Range {
                        start: to_location(text),
                        end: to_location(text),
                    }],
                }],
            })
        }
        nom::Err::Incomplete(_) => None,
    }
}

fn convert_error(input: &str, err: VerboseError<Text>) -> String {
    if let Some((text, _error_kind)) = err.errors.get(0) {
        let error = crate::error::Error {
            filepath: text.extra.file.to_string(),
            error_type: crate::error::ErrorType::ParsingError(crate::error::ParsingErrorDetails {
                expecting: text.extra.expecting.clone(),
            }),
            locations: vec![crate::error::Location {
                contexts: vec![],
                primary: vec![crate::error::Range {
                    start: to_location(text),
                    end: to_location(text),
                }],
            }],
        };

        crate::error::format_error(input, &error)
    } else {
        "No errors".to_string()
    }
}

fn parse_schema<'a>(input: Text<'a>, schema: &'a mut ast::Schema) -> ParseResult<'a, ()> {
    let (input, _) = multispace0(input)?;
    let (input, definitions) = many1(parse_definition)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = eof(input)?;
    add_session(schema, &definitions);

    let file = ast::SchemaFile {
        path: input.extra.file.to_string(),
        definitions,
    };

    schema.files.push(file);
    Ok((input, ()))
}

fn add_session(schema: &mut ast::Schema, definitions: &Vec<ast::Definition>) {
    for definition in definitions {
        match definition {
            ast::Definition::Session(session) => {
                schema.session = Some(session.clone());
                return;
            }
            _ => (),
        }
    }
}

fn parse_definition(input: Text) -> ParseResult<ast::Definition> {
    alt((
        parse_comment,
        parse_tagged,
        parse_record,
        parse_session,
        parse_lines,
    ))(input)
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
    let (input, val) = recognize(tuple((
        take_while1(|c: char| c.is_uppercase()), // First character needs to be uppercase
        many0(alt((alphanumeric1, take_while1(|c: char| c == '_')))), // Followed by alphanumeric or underscores
    )))(input)?;

    Ok((input, val.fragment()))
}

fn parse_fieldname(input: Text) -> ParseResult<&str> {
    let (input, val) = recognize(tuple((
        alt((
            take_while1(|c: char| c.is_lowercase()), // First character must be lowercase
            take_while1(|c: char| c == '_'),         // or an underscore
        )),
        many0(alt((alphanumeric1, take_while1(|c: char| c == '_')))), // Followed by alphanumeric or underscores
    )))(input)?;

    Ok((input, val.fragment()))
}

fn parse_qualified(input: Text) -> ParseResult<ast::Qualified> {
    let (input, first) = parse_typename(input)?;
    let (input, _) = tag(".")(input)?;

    // Try to parse just a fieldname first
    let (input, result) = alt((
        // Try fieldname only
        |input| {
            let (input, field) = parse_fieldname(input)?;
            Ok((input, (None, field)))
        },
        // Try typename + fieldname
        |input| {
            let (input, table) = parse_typename(input)?;
            let (input, _) = tag(".")(input)?;
            let (input, field) = parse_fieldname(input)?;
            Ok((input, (Some(table), field)))
        },
    ))(input)?;

    match result {
        (Some(table), field) => {
            // Got schema.table.field
            Ok((
                input,
                ast::Qualified {
                    schema: first.to_string(),
                    table: table.to_string(),
                    fields: vec![field.to_string()],
                },
            ))
        }
        (None, field) => {
            let namespace = input.extra.namespace.to_string();
            // Got table.field
            Ok((
                input,
                ast::Qualified {
                    schema: namespace,
                    table: first.to_string(),
                    fields: vec![field.to_string()],
                },
            ))
        }
    }
}

fn parse_record(input: Text) -> ParseResult<ast::Definition> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("record")(input)?;
    let (input, _) = cut(space1)(input)?;
    let (input, start_name_pos) = position(input)?;
    let (input, name) = cut(parse_typename)(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = cut(multispace0)(input)?;
    let (input, fields) = cut(with_braces(parse_field))(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = alt((line_ending, eof))(input)?;

    Ok((
        input,
        ast::Definition::Record {
            name: name.to_string(),
            fields,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),
            start_name: Some(to_location(&start_name_pos)),
            end_name: Some(to_location(&end_name_pos)),
        },
    ))
}

fn parse_session(input: Text) -> ParseResult<ast::Definition> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("session")(input)?;
    let (input, _) = cut(multispace1)(input)?;
    let (input, fields) = cut(with_braces(parse_field))(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = alt((line_ending, eof))(input)?;

    Ok((
        input,
        ast::Definition::Session(ast::SessionDetails {
            fields,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),
        }),
    ))
}

fn parse_field(input: Text) -> ParseResult<ast::Field> {
    alt((
        parse_field_comment,
        parse_table_directive,
        parse_column,
        parse_column_lines,
    ))(input)
}

fn parse_column_lines(input: Text) -> ParseResult<ast::Field> {
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::Field::ColumnLines { count }))
}

fn parse_field_comment(input: Text) -> ParseResult<ast::Field> {
    let (input, _) = tag("//")(input)?;
    let (input, text) = cut(take_until("\n"))(input)?;
    let (input, _) = alt((line_ending, eof))(input)?;
    Ok((
        input,
        ast::Field::ColumnComment {
            text: text.to_string(),
        },
    ))
}

fn parse_table_directive(input: Text) -> ParseResult<ast::Field> {
    let (input, start_pos) = position(input)?;
    let input = expecting(input, crate::error::Expecting::SchemaAtDirective);
    let (input, _) = tag("@")(input)?;

    let (input, field_directive) = cut(alt((
        parse_watch(),
        parse_tablename(to_location(&start_pos)),
        parse_table_permission,
        parse_link,
    )))(input)?;
    let input = expecting(input, crate::error::Expecting::SchemaColumn);
    Ok((input, field_directive))
}

fn parse_table_permission(input: Text) -> ParseResult<ast::Field> {
    let (input, _) = tag("permissions")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("{")(input)?;
    let (input, _) = multispace0(input)?;

    // Parse either a star permission or operation-specific permissions
    let (input, details) = alt((
        // Star permission case
        |input| {
            let (input, where_) = parse_where_arg(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = tag("}")(input)?;
            Ok((input, ast::PermissionDetails::Star(where_)))
        },
        // Operation-specific permissions case
        |input| {
            let (input, operations) = separated_list0(newline, |input| {
                let (input, _) = multispace0(input)?;
                let (input, ops) = separated_list1(
                    tag(","),
                    alt((
                        value(ast::QueryOperation::Select, tag("select")),
                        value(ast::QueryOperation::Insert, tag("insert")),
                        value(ast::QueryOperation::Update, tag("update")),
                        value(ast::QueryOperation::Delete, tag("delete")),
                    )),
                )(input)?;
                let (input, _) = multispace0(input)?;
                let (input, where_) = with_comma_sep_braces(parse_where_arg)(input)?;

                let where_ = if where_.len() == 1 {
                    where_.into_iter().next().unwrap()
                } else {
                    ast::WhereArg::And(where_)
                };
                Ok((
                    input,
                    ast::PermissionOnOperation {
                        operations: ops,
                        where_: where_,
                    },
                ))
            })(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = tag("}")(input)?;
            Ok((input, ast::PermissionDetails::OnOperation(operations)))
        },
    ))(input)?;

    let (input, _) = multispace0(input)?;
    let (input, _) = newline(input)?;

    Ok((
        input,
        ast::Field::FieldDirective(ast::FieldDirective::Permissions(details)),
    ))
}

fn parse_watch() -> impl Fn(Text) -> ParseResult<ast::Field> {
    move |input: Text| {
        let (input, _) = tag("watch")(input)?;
        let (input, _) = multispace0(input)?;

        let directive = ast::FieldDirective::Watched(ast::WatchedDetails {
            selects: false,
            inserts: true,
            updates: false,
            deletes: false,
        });
        return Ok((input, ast::Field::FieldDirective(directive)));
    }
}

fn parse_tablename(start_location: ast::Location) -> impl Fn(Text) -> ParseResult<ast::Field> {
    move |input: Text| {
        let (input, _) = tag("tablename")(input)?;
        let (input, _) = multispace1(input)?;
        let (input, tablename) = parse_string_literal(input)?;
        let (input, end_pos) = position(input)?;
        let (input, _) = multispace0(input)?;

        let range = ast::Range {
            start: start_location.clone(),
            end: to_location(&end_pos),
        };

        let directive = ast::FieldDirective::TableName((range, tablename.to_string()));
        return Ok((input, ast::Field::FieldDirective(directive)));
    }
}

// This is deprecated and will be moved pretty quick
fn parse_link(input: Text) -> ParseResult<ast::Field> {
    let (input, _) = tag("link")(input)?;
    let input = expecting(input, crate::error::Expecting::LinkDirective);
    let (input, _) = cut(multispace1)(input)?;
    let (input, start_pos) = position(input)?;
    let (input, linkname) = parse_fieldname(input)?;
    let (input, end_name_pos) = position(input)?;

    // link details
    // { from: userId, to: User.id }
    let (input, _) = multispace1(input)?;
    let (input, fields) = with_braces(parse_link_field)(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = newline(input)?;

    // gather into link details
    let (input, link_details) =
        link_field_to_details(input, linkname, start_pos, end_name_pos, fields)?;

    return Ok((
        input,
        ast::Field::FieldDirective(ast::FieldDirective::Link(link_details)),
    ));
}

#[derive(Debug, Clone, PartialEq)]
enum LinkField {
    From(Vec<String>),
    To { table: String, id: Vec<String> },
}

fn link_field_to_details<'a, 'b>(
    input: Text<'a>,
    linkname: &'b str,
    start_pos: Text<'a>,
    end_name_pos: Text<'a>,
    link_fields: Vec<LinkField>,
) -> ParseResult<'a, ast::LinkDetails> {
    let mut details = ast::LinkDetails {
        link_name: linkname.to_string(),
        local_ids: vec![],

        foreign: ast::Qualified {
            schema: "".to_string(),
            table: "".to_string(),
            fields: vec![],
        },

        start_name: Some(to_location(&start_pos)),
        end_name: Some(to_location(&end_name_pos)),
    };
    let mut has_from = false;
    let mut has_to = false;
    for link in link_fields {
        match link {
            LinkField::From(id_list) => {
                if has_from {
                    return Err(nom::Err::Error(VerboseError {
                        errors: vec![(input, VerboseErrorKind::Context("tag"))],
                    }));
                } else {
                    has_from = true;
                    details.local_ids = id_list
                }
            }
            LinkField::To { table, id } => {
                if has_to {
                    return Err(nom::Err::Error(VerboseError {
                        errors: vec![(input, VerboseErrorKind::Context("tag"))],
                    }));
                } else {
                    has_to = true;
                    details.foreign.table = table;
                    details.foreign.fields = id;
                }
            }
        }
    }
    if has_to & has_from {
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
            return Err(nom::Err::Error(VerboseError {
                errors: vec![(input, VerboseErrorKind::Context("tag"))],
            }));
        }
    }
}

fn to_location(pos: &Text) -> ast::Location {
    ast::Location {
        offset: pos.location_offset(),
        line: pos.location_line(),
        column: pos.get_column(),
    }
}
fn parse_column(input: Text) -> ParseResult<ast::Field> {
    let (input, start_pos) = position(input)?;
    let input = expecting(input, crate::error::Expecting::SchemaColumn);
    let (input, name) = parse_fieldname(input)?;
    let (input, end_name_pos) = position(input)?;

    let (input, _) = space0(input)?;
    let (input, maybe_link) = cut(opt(tag("@link")))(input)?;

    if let Some(_) = maybe_link {
        let (input, _) = cut(tag("("))(input)?;
        let input = expecting(input, crate::error::Expecting::LinkDirective);
        // We're parsing either
        //          fieldname @link(local_id, ForeignTable.foreignId)
        //          fieldname @link(ForeignTable.foreignId)
        let (input, first_arg) = opt(|input| {
            let (input, first_arg) = parse_fieldname(input)?;
            let (input, _) = tag(",")(input)?;
            let (input, _) = space0(input)?;
            Ok((input, first_arg.to_string()))
        })(input)?;

        let (input, foreign) = parse_qualified(input)?;
        let link_details = ast::LinkDetails {
            link_name: name.to_string(),
            local_ids: vec![first_arg.unwrap_or("id".to_string())],

            foreign: foreign,
            start_name: Some(to_location(&start_pos)),
            end_name: Some(to_location(&end_name_pos)),
        };
        let (input, _) = tag(")")(input)?;
        let (input, _) = space0(input)?;
        let (input, _) = newline(input)?;
        let (input, _) = space0(input)?;

        return Ok((
            input,
            ast::Field::FieldDirective(ast::FieldDirective::Link(link_details)),
        ));
    };

    let (input, _) = cut(opt(tag(":")))(input)?;
    let (input, _) = space0(input)?;
    let (input, start_type_pos) = position(input)?;
    let (input, type_) = cut(parse_typename)(input)?;
    let (input, end_type_pos) = position(input)?;
    let (input, is_nullable) = parse_nullable(input)?;
    let (input, _) = space0(input)?;
    let (input, directives) = many0(parse_column_directive)(input)?;

    let (input, end_pos) = position(input)?;

    let (input, _) = space0(input)?;
    let (input, _) = newline(input)?;
    let (input, _) = space0(input)?;

    Ok((
        input,
        ast::Field::Column(ast::Column {
            name: name.to_string(),
            type_: type_.to_string(),
            nullable: is_nullable,
            serialization_type: platform::to_serialization_type(type_),
            directives,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),

            start_name: Some(to_location(&start_pos)),
            end_name: Some(to_location(&end_name_pos)),

            start_typename: Some(to_location(&start_type_pos)),
            end_typename: Some(to_location(&end_type_pos)),
        }),
    ))
}

fn parse_nullable(input: Text) -> ParseResult<bool> {
    let (input, maybe_nullable) = opt(char('?'))(input)?;
    Ok((input, maybe_nullable != None))
}

fn parse_column_directive(input: Text) -> ParseResult<ast::ColumnDirective> {
    let (input, _) = tag("@")(input)?;
    let input = expecting(input, crate::error::Expecting::SchemaFieldAtDirective);
    cut(alt((
        parse_directive_named("id", ast::ColumnDirective::PrimaryKey),
        parse_directive_named("unique", ast::ColumnDirective::Unique),
        parse_default_directive,
    )))(input)
}

fn parse_default_directive(input: Text) -> ParseResult<ast::ColumnDirective> {
    let (input, _) = tag("default(")(input)?;
    let (input, _) = space0(input)?;
    let (input, default) = alt((
        parse_token("now", ("now".to_string(), ast::DefaultValue::Now)),
        parse_default_value,
    ))(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((
        input,
        ast::ColumnDirective::Default {
            id: default.0,
            value: default.1,
        },
    ))
}

fn parse_default_value(input: Text) -> ParseResult<(String, ast::DefaultValue)> {
    let start_offset = input.location_offset();
    let (input_after, val) = parse_value(input)?;
    let end_offset = input_after.location_offset();

    let original_text = &input_after.fragment()[..end_offset - start_offset];
    let id = original_text.to_string();

    Ok((input_after, (id, ast::DefaultValue::Value(val))))
}

fn parse_directive_named<'a, T>(tag_str: &'a str, value: T) -> impl Fn(Text) -> ParseResult<T> + 'a
where
    T: Clone + 'a,
{
    move |input: Text| {
        let (input, _) = tag(tag_str)(input)?;
        Ok((input, value.clone()))
    }
}

fn parse_type_separator(input: Text) -> ParseResult<char> {
    delimited(multispace0, char('|'), multispace0)(input)
}

fn parse_tagged(input: Text) -> ParseResult<ast::Definition> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("type")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = cut(parse_typename)(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, variants) = separated_list0(parse_type_separator, parse_variant)(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = alt((line_ending, eof))(input)?;

    Ok((
        input,
        ast::Definition::Tagged {
            name: name.to_string(),
            variants,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),
        },
    ))
}

fn parse_variant(input: Text) -> ParseResult<ast::Variant> {
    let (input, start_pos) = position(input)?;
    let (input, name) = parse_typename(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = multispace0(input)?;
    let (input, optional_fields) = opt(with_braces(parse_field))(input)?;
    let (input, end_pos) = position(input)?;

    Ok((
        input,
        ast::Variant {
            name: name.to_string(),
            fields: optional_fields,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),

            start_name: Some(to_location(&start_pos)),
            end_name: Some(to_location(&end_name_pos)),
        },
    ))
}

// Parse Query
//

pub fn parse_query<'a>(
    path: &'a str,
    input_str: &'a str,
) -> Result<ast::QueryList, nom::Err<VerboseError<Text<'a>>>> {
    let input = Text::new_extra(
        input_str,
        ParseContext {
            file: path,
            namespace: "queries_dont_need_a_default_namespace".to_string(),
            expecting: crate::error::Expecting::PyreFile,
        },
    );
    match parse_query_list(input) {
        Ok((_remaining, query_list)) => {
            return Ok(query_list);
        }
        Err(e) => Err(e),
    }
}

fn parse_query_list(input: Text) -> ParseResult<ast::QueryList> {
    let (input, _) = multispace0(input)?;
    let (input, queries) = all_consuming(many1(parse_query_def))(input)?;
    Ok((input, ast::QueryList { queries }))
}

fn parse_query_def(input: Text) -> ParseResult<ast::QueryDef> {
    alt((parse_query_comment, parse_query_lines, parse_query_details))(input)
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

    let (input, _) = opt(eof)(input)?; // Ensure we are at the end of the file (or query list

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
    let (input, _) = cut(multispace1)(input)?;
    let (input, start_pos) = position(input)?;
    let (input, name) = cut(parse_typename)(input)?;

    let (input, param_defs_or_nothing) = cut(opt(parse_query_param_definitions))(input)?;
    let (input, _) = space0(input)?;
    let (input, fields) = with_braces(parse_toplevel_query_field)(input)?;
    let (input, end_pos) = position(input)?;
    let (input, _) = opt(newline)(input)?;

    let mut query = ast::Query {
        interface_hash: "".to_string(),
        full_hash: "".to_string(),
        operation: op,
        name: name.to_string(),
        args: param_defs_or_nothing.unwrap_or(vec![]),
        fields,
        start: Some(to_location(&start_pos)),
        end: Some(to_location(&end_pos)),
    };
    let interface_hash = crate::hash::hash_query_interface(&query);
    let full_hash = crate::hash::hash_query_full(&query);

    query.interface_hash = interface_hash;
    query.full_hash = full_hash;

    Ok((input, ast::QueryDef::Query(query)))
}

// TOP LEVEL QUERY FIELD PARSING

fn parse_toplevel_query_field(input: Text) -> ParseResult<ast::TopLevelQueryField> {
    alt((
        parse_toplevel_query_comment,
        parse_toplevel_querydetails_field,
        parse_toplevel_query_lines,
    ))(input)
}

fn parse_toplevel_querydetails_field(input: Text) -> ParseResult<ast::TopLevelQueryField> {
    let (input, q) = parse_query_field(input)?;
    let (input, _) = opt(newline)(input)?;
    Ok((input, ast::TopLevelQueryField::Field(q)))
}

fn parse_toplevel_query_lines(input: Text) -> ParseResult<ast::TopLevelQueryField> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::TopLevelQueryField::Lines { count }))
}

fn parse_toplevel_query_comment(input: Text) -> ParseResult<ast::TopLevelQueryField> {
    let (input, _) = tag("//")(input)?;
    let (input, text) = take_until("\n")(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::TopLevelQueryField::Comment {
            text: text.to_string(),
        },
    ))
}

// QUERY PARAMETER PARSING

fn parse_query_param_definitions(input: Text) -> ParseResult<Vec<ast::QueryParamDefinition>> {
    let (input, _) = tag("(")(input)?;
    let input = expecting(input, crate::error::Expecting::ParamDefinition);
    let (input, _) = cut(multispace0)(input)?;
    let (input, fields) = cut(separated_list1(char(','), parse_query_param_definition))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, fields))
}

fn parse_query_param_definition(input: Text) -> ParseResult<ast::QueryParamDefinition> {
    let (input, _) = multispace0(input)?;
    let (input, start_name_pos) = position(input)?;
    let (input, _) = tag("$")(input)?;
    let input = expecting(input, crate::error::Expecting::ParamDefType);
    let (input, name) = cut(parse_fieldname)(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = space0(input)?;

    let (input, _) = opt(char(':'))(input)?;
    let (input, _) = space0(input)?;

    let (input, start_type_pos) = position(input)?;
    let (input, typename) = cut(opt(parse_typename))(input)?;
    let (input, end_type_pos) = position(input)?;
    let (input, _) = multispace0(input)?;

    Ok((
        input,
        ast::QueryParamDefinition {
            name: name.to_string(),
            type_: typename.map(|t| t.to_string()),
            start_name: Some(to_location(&start_name_pos)),
            end_name: Some(to_location(&end_name_pos)),
            start_type: Some(to_location(&start_type_pos)),
            end_type: Some(to_location(&end_type_pos)),
        },
    ))
}

fn parse_block_separator(input: Text) -> ParseResult<()> {
    alt((value((), char(',')), value((), newline)))(input)
}

fn with_comma_sep_braces<'a, F, T>(parse_term: F) -> impl Fn(Text) -> ParseResult<Vec<T>>
where
    F: Fn(Text) -> ParseResult<T>,
{
    move |input: Text| {
        let (input, _) = tag("{")(input)?;
        let (input, _) = multispace0(input)?;
        let (input, terms) = separated_list0(parse_block_separator, &parse_term)(input)?;
        let (input, _) = multispace0(input)?;
        let (input, _) = tag("}")(input)?;
        Ok((input, terms))
    }
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
    alt((
        parse_inline_query_comment,
        parse_query_arg_field,
        parse_arg,
        parse_inline_query_lines,
    ))(input)
}

fn parse_inline_query_lines(input: Text) -> ParseResult<ast::ArgField> {
    // Parse any whitespace (spaces, tabs, or newlines)
    let (input, whitespaces) = many1(one_of(" \t\n"))(input)?;

    // Count the newlines
    let count = whitespaces.iter().filter(|&&c| c == '\n').count();

    Ok((input, ast::ArgField::Lines { count }))
}

fn parse_inline_query_comment(input: Text) -> ParseResult<ast::ArgField> {
    let (input, _) = tag("//")(input)?;
    let (input, text) = take_until("\n")(input)?;
    let (input, _) = newline(input)?;
    Ok((
        input,
        ast::ArgField::QueryComment {
            text: text.to_string(),
        },
    ))
}

fn parse_arg(input: Text) -> ParseResult<ast::ArgField> {
    let (input, _) = multispace0(input)?;
    let (input, start_pos) = position(input)?;
    let (input, arg) = parse_query_arg(input)?;
    let (input, end_pos) = position(input)?;
    Ok((
        input,
        ast::ArgField::Arg(ast::LocatedArg {
            arg,
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),
        }),
    ))
}

fn parse_query_arg_field(input: Text) -> ParseResult<ast::ArgField> {
    let (input, q) = parse_query_field(input)?;
    let (input, _) = opt(newline)(input)?;
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
    let (input, fields_or_none) = opt(with_braces(parse_arg_field))(input)?;
    let (input, end_pos) = position(input)?;
    let input = expecting(input, crate::error::Expecting::PyreFile);
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
            fields: fields_or_none.unwrap_or_else(Vec::new),
            start: Some(to_location(&start_pos)),
            end: Some(to_location(&end_pos)),

            start_fieldname: Some(to_location(&start_pos)),
            end_fieldname: Some(to_location(&end_fieldname_pos)),
        },
    ))
}

fn parse_query_arg(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = tag("@")(input)?;
    let input = expecting(input, crate::error::Expecting::AtDirective);
    // cut(alt((parse_limit, parse_offset, parse_sort, parse_where)))(input)
    cut(alt((parse_sort, parse_where)))(input)
}

fn parse_limit(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = tag("limit")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;

    Ok((input, ast::Arg::Limit(val)))
}

fn parse_offset(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("offset")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, ast::Arg::Offset(val)))
}

fn parse_sort(input: Text) -> ParseResult<ast::Arg> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("sort")(input)?;
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
    let (input, _) = tag("where")(input)?;
    let (input, _) = space1(input)?;
    let (input, where_arg) = with_comma_sep_braces(parse_where_arg)(input)?;

    if where_arg.len() == 1 {
        Ok((
            input,
            ast::Arg::Where(where_arg.into_iter().next().unwrap()),
        ))
    } else {
        Ok((input, ast::Arg::Where(ast::WhereArg::And(where_arg))))
    }
}

fn parse_where_arg(input: Text) -> ParseResult<ast::WhereArg> {
    let (input, _) = multispace0(input)?;
    let (input, where_col) = parse_query_where(input)?;
    let (input, _) = space0(input)?;
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
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("$")(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, end_pos) = position(input)?;
    let range = ast::Range {
        start: to_location(&start_pos),
        end: to_location(&end_pos),
    };
    Ok((
        input,
        ast::QueryValue::Variable((
            range,
            ast::VariableDetails {
                session_field: None,
                name: name.to_string(),
            },
        )),
    ))
}

fn parse_fn(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, start_pos) = position(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, end_name_pos) = position(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, start_args_pos) = position(input)?;
    let (input, args) = separated_list1(char(','), parse_value)(input)?;
    let (input, end_args_pos) = position(input)?;
    let (input, _) = tag(")")(input)?;
    let (input, end_pos) = position(input)?;

    let range = ast::Range {
        start: to_location(&start_pos),
        end: to_location(&end_pos),
    };
    Ok((
        input,
        ast::QueryValue::Fn(ast::FnDetails {
            name: name.to_string(),
            args,
            location: range,
            location_fn_name: ast::Range {
                start: to_location(&start_pos),
                end: to_location(&end_name_pos),
            },
            location_arg: ast::Range {
                start: to_location(&start_args_pos),
                end: to_location(&end_args_pos),
            },
        }),
    ))
}

fn parse_session_variable(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, start_pos) = position(input)?;
    let (input, _) = tag("Session.")(input)?;
    let (input, name) = parse_fieldname(input)?;
    let (input, end_pos) = position(input)?;
    let range = ast::Range {
        start: to_location(&start_pos),
        end: to_location(&end_pos),
    };
    Ok((
        input,
        ast::QueryValue::Variable((
            range,
            ast::VariableDetails {
                session_field: Some(name.to_string()),
                name: format!("session_{}", name.to_string()),
            },
        )),
    ))
}

fn parse_value(input: Text) -> ParseResult<ast::QueryValue> {
    alt((
        parse_located_token("null", |range| ast::QueryValue::Null(range)),
        parse_located_token("Null", |range| ast::QueryValue::Null(range)),
        parse_located_token("True", |range| ast::QueryValue::Bool((range, true))),
        parse_located_token("False", |range| ast::QueryValue::Bool((range, false))),
        parse_located_token("true", |range| ast::QueryValue::Bool((range, true))),
        parse_located_token("false", |range| ast::QueryValue::Bool((range, false))),
        parse_variable,
        parse_session_variable,
        parse_typed_value,
        parse_string,
        parse_number,
        parse_fn,
    ))(input)
}

fn parse_typed_value(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, start_pos) = position(input)?;
    let (input, variant_name) = parse_typename(input)?;
    let (input, end_pos) = position(input)?;
    let range = ast::Range {
        start: to_location(&start_pos),
        end: to_location(&end_pos),
    };
    Ok((
        input,
        ast::QueryValue::LiteralTypeValue((
            range,
            ast::LiteralTypeValueDetails {
                name: variant_name.to_string(),
            },
        )),
    ))
}

//
// Parses the first digits, starting with any character except 0
// If there is a ., then it must be a `ast::Float`, otherwise it's an ast::Int.
pub fn parse_number(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, start_pos) = position(input)?;
    let (input, first) = one_of("1234567890")(input)?;
    if first == '0' {
        let (input, end_pos) = position(input)?;
        let range = ast::Range {
            start: to_location(&start_pos),
            end: to_location(&end_pos),
        };
        Ok((input, ast::QueryValue::Int((range, 0))))
    } else {
        let (input, rest) = take_while(|c: char| c.is_digit(10))(input)?;
        let (input, dot) = opt(tag("."))(input)?;
        let (input, tail) = take_while(|c: char| c.is_digit(10))(input)?;
        let (input, end_pos) = position(input)?;

        let range = ast::Range {
            start: to_location(&start_pos),
            end: to_location(&end_pos),
        };

        let period = if dot.is_some() { "." } else { "" };

        let value = format!("{}{}{}{}", first, rest, period, tail);
        if value.contains(".") {
            Ok((
                input,
                ast::QueryValue::Float((range, value.parse().unwrap())),
            ))
        } else {
            Ok((input, ast::QueryValue::Int((range, value.parse().unwrap()))))
        }
    }
}

fn parse_string(input: Text) -> ParseResult<ast::QueryValue> {
    let (input, start_pos) = position(input)?;
    let (input, value) = parse_string_literal(input)?;
    let (input, end_pos) = position(input)?;

    let range = ast::Range {
        start: to_location(&start_pos),
        end: to_location(&end_pos),
    };
    Ok((input, ast::QueryValue::String((range, value.to_string()))))
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

fn parse_located_token<'a, T, F>(
    tag_str: &'a str,
    value_constructor: F,
) -> impl Fn(Text) -> ParseResult<T> + 'a
where
    T: Clone + 'a,
    F: Fn(ast::Range) -> T + 'a,
{
    move |input: Text| {
        let (input, start_pos) = position(input)?;
        let (input, _) = tag(tag_str)(input)?;
        let (input, end_pos) = position(input)?;

        let range = ast::Range {
            start: to_location(&start_pos),
            end: to_location(&end_pos),
        };

        let value = value_constructor(range);
        Ok((input, value))
    }
}
