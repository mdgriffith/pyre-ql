use crate::ast;
use crate::ext::string;
use crate::typecheck;
pub struct TypeFormatter {
    pub to_comment: Box<dyn Fn(&str) -> String>,
    pub to_type_def_start: Box<dyn Fn(&str) -> String>,
    pub to_field: Box<dyn Fn(&str, &str, FieldMetadata) -> String>,
    pub to_type_def_end: Box<dyn Fn() -> String>,
    pub to_field_separator: Box<dyn Fn(bool) -> String>,
}

pub struct FieldMetadata {
    pub is_link: bool,
    pub is_optional: bool,
}

/// Generates type alias definitions for query return types using the provided formatting functions
///
/// # Example
/// ```rust
/// let elm_formatter = TypeFormatter {
///     to_comment: Box::new(|s| format!("{{-| {} -}}\n", s)),
///     to_type_def_start: Box::new(|name| format!("type alias {} =\n", name)),
///     to_field: Box::new(|name, type_, is_list| {
///         let type_str = if is_list { format!("List {}", type_) } else { type_.to_string() };
///         format!("    {} : {}", name, type_str)
///     }),
///     to_type_def_end: Box::new(|| "    }\n".to_string()),
///     to_field_separator: Box::new(|| ",\n".to_string()),
/// };
///
/// let mut result = String::new();
/// return_data_aliases(&context, &query, &mut result, &elm_formatter);
/// ```
pub fn return_data_aliases(
    context: &typecheck::Context,
    query: &ast::Query,
    result: &mut String,
    formatter: &TypeFormatter,
) {
    // Add comment and type definition start
    result.push_str(&(formatter.to_comment)("The Return Data!"));

    // Children aliases
    for field in &query.fields {
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let table = context.tables.get(&query_field.name).unwrap();
                to_query_type_alias(context, &table.record, "", query_field, formatter, result);
            }
            ast::TopLevelQueryField::Lines { .. } => {}
            ast::TopLevelQueryField::Comment { .. } => {}
        }
    }

    // Global Return Data Alias
    result.push_str(&(formatter.to_type_def_start)(
        &crate::ext::string::capitalize("ReturnData"),
    ));

    for (i, field) in query.fields.iter().enumerate() {
        let is_last = i == query.fields.len() - 1;
        match field {
            ast::TopLevelQueryField::Field(query_field) => {
                let field_name: String = ast::get_aliased_name(query_field);

                result.push_str(&(formatter.to_field)(
                    &crate::ext::string::decapitalize(&field_name),
                    &string::capitalize(&field_name),
                    FieldMetadata {
                        is_link: true,
                        is_optional: false,
                    },
                ));

                result.push_str(&(formatter.to_field_separator)(is_last));
            }
            _ => {}
        }
    }

    result.push_str(&(formatter.to_type_def_end)());
    result.push_str("\n\n");
}

fn get_name(alias_stack: &str, field_name: &str) -> String {
    if alias_stack.is_empty() {
        crate::ext::string::capitalize(field_name)
    } else {
        format!(
            "{}_{}",
            alias_stack,
            crate::ext::string::capitalize(field_name)
        )
    }
}

fn to_query_type_alias(
    context: &typecheck::Context,
    table: &ast::RecordDetails,
    alias_stack: &str,
    query_field: &ast::QueryField,
    formatter: &TypeFormatter,
    //
    result: &mut String,
) {
    let child_alias_stack = push_alias_stack(query_field, alias_stack);
    // Children first
    let fields = &ast::collect_query_fields(&query_field.fields);
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

                to_query_type_alias(
                    context,
                    &link_table.record,
                    &child_alias_stack,
                    field,
                    formatter,
                    result,
                );
            }
            _ => continue,
        }
    }

    // Local Return Data Alias
    result.push_str(&(formatter.to_type_def_start)(&get_name(
        alias_stack,
        &ast::get_aliased_name(query_field),
    )));

    let alias_stack = push_alias_stack(query_field, alias_stack);

    for (i, field) in fields.iter().enumerate() {
        let is_last = i == fields.len() - 1;

        let table_field = &table
            .fields
            .iter()
            .find(|&f| ast::has_field_or_linkname(&f, &field.name))
            .unwrap();

        let aliased_name = ast::get_aliased_name(field);

        match table_field {
            ast::Field::Column(col) => {
                result.push_str(&(formatter.to_field)(
                    &aliased_name,
                    &col.type_,
                    FieldMetadata {
                        is_link: false,
                        is_optional: col.nullable,
                    },
                ));
                result.push_str(&(formatter.to_field_separator)(is_last));
            }
            ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                let linked_to_unique = ast::linked_to_unique_field(&link);
                // If we're linked to a unqiue field
                // Then we have either 0 or 1 of them

                result.push_str(&(formatter.to_field)(
                    &aliased_name,
                    &get_name(&alias_stack, &aliased_name),
                    FieldMetadata {
                        is_link: true,
                        is_optional: linked_to_unique,
                    },
                ));
                result.push_str(&(formatter.to_field_separator)(is_last));
            }
            _ => {}
        }
    }

    result.push_str(&(formatter.to_type_def_end)());
    result.push_str("\n\n");
}

pub fn push_alias_stack(field: &ast::QueryField, alias_stack: &str) -> String {
    let name = field.alias.as_ref().unwrap_or(&field.name);
    let capitalized = crate::ext::string::capitalize(name);

    if alias_stack.is_empty() {
        capitalized
    } else {
        format!("{}_{}", alias_stack, capitalized)
    }
}
