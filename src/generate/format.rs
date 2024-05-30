use crate::ast;

pub fn to_string(schema: &ast::Schema) -> String {
    let mut result = String::new();
    for definition in &schema.definitions {
        result.push_str(&to_string_definition(definition));
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
        ast::Definition::Comment { text } => format!("// {}\n", text),
        ast::Definition::Tagged { name, variants } => {
            let mut result = format!("type {}\n", name);
            let mut is_first = true;
            for variant in variants {
                result.push_str(&to_string_variant(is_first, variant));
                is_first = false;
            }
            result
        }
        ast::Definition::Record { name, fields } => {
            let mut result = format!("record {} {{\n", name);
            for field in fields {
                result.push_str(&to_string_field(4, field));
            }
            result.push_str("}\n");
            result
        }
    }
}

fn to_string_variant(is_first: bool, variant: &ast::Variant) -> String {
    let prefix = if is_first { " = " } else { " | " };

    match &variant.data {
        Some(fields) => {
            let mut result = format!("  {}{} {{\n", prefix, variant.name);
            for field in fields {
                result.push_str(&to_string_field(8, field));
            }
            result.push_str("     }\n");
            result
        }
        None => format!("  {}{}\n", prefix, variant.name),
    }
}

fn to_string_field(indent: usize, field: &ast::Field) -> String {
    let spaces = " ".repeat(indent);
    format!("{}{}: {}\n", spaces, field.name, field.type_)
}
