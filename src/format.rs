use crate::ast;
use std::collections::HashMap;

pub fn schema(schem: &mut ast::Schema) {
    // Insert some lines before each definition if needed
    let mut i = 0;
    let mut prev_was_lines = false;

    while i < schem.definitions.len() {
        if let ast::Definition::Lines { .. } = schem.definitions[i] {
            prev_was_lines = true;
        } else if !prev_was_lines {
            schem
                .definitions
                .insert(i, ast::Definition::Lines { count: 1 });
            // Move to the next element after insertion
            i += 1;
        } else {
            prev_was_lines = false;
        }
        i += 1;
    }

    let mut links: HashMap<String, Vec<(bool, ast::LinkDetails)>> = HashMap::new();
    // Get all links and calculate reciprocals
    for def in &schem.definitions {
        if let ast::Definition::Record {
            name,
            fields,
            start,
            end,
            start_name,
            end_name,
        } = def
        {
            // let tablename = ast::get_tablename(name, fields);
            for field in fields {
                match field {
                    ast::Field::FieldDirective(ast::FieldDirective::Link(link)) => {
                        add_link(&mut links, &name, &link, true);
                        let reciprocal = ast::to_reciprocal(&name, link);
                        add_link(&mut links, &link.foreign_tablename, &reciprocal, false);
                    }
                    _ => (),
                }
            }
        }
    }

    // Standard formatting
    for definition in &mut schem.definitions {
        format_definition(definition, &links);
    }
}

fn add_link(
    links: &mut HashMap<String, Vec<(bool, ast::LinkDetails)>>,
    tablename: &str,
    link: &ast::LinkDetails,
    is_calculated: bool,
) {
    match links.get(tablename) {
        None => {
            links.insert(tablename.to_string(), vec![(is_calculated, link.clone())]);
        }
        Some(existing_links) => {
            enum LinkOp {
                Append,
                Skip,
                Replace(usize),
            }
            let mut op = LinkOp::Append;
            for (i, (existing_calculated, existing_link)) in existing_links.iter().enumerate() {
                if ast::link_equivalent(link, existing_link) {
                    if (is_calculated) {
                        // The new link is calculated
                        // So we should skip this because it's already been added
                        op = LinkOp::Skip
                    } else if (*existing_calculated) {
                        op = LinkOp::Replace(i);
                    }
                }
            }
            match op {
                LinkOp::Append => {
                    let mut new_links = existing_links.clone();
                    new_links.push((is_calculated, link.clone()));
                    links.insert(tablename.to_string(), new_links);
                }
                LinkOp::Skip => (),
                LinkOp::Replace(i) => {
                    let mut new_links = existing_links.clone();
                    new_links[i] = (is_calculated, link.clone());
                    links.insert(tablename.to_string(), new_links);
                }
            }
        }
    }
}

fn format_definition(
    def: &mut ast::Definition,
    links: &HashMap<String, Vec<(bool, ast::LinkDetails)>>,
) {
    match def {
        ast::Definition::Lines { count } => {
            *count = std::cmp::max(1, std::cmp::min(*count, 2));
        }
        ast::Definition::Comment { text } => (),
        ast::Definition::Tagged {
            name,
            variants,
            start,
            end,
        } => (),
        ast::Definition::Record {
            name,
            ref mut fields,
            start,
            end,
            start_name,
            end_name,
        } => {
            fields.retain(|field| !ast::is_link(field));

            match links.get(name) {
                Some(all_links) => {
                    for (is_calculated, link) in all_links {
                        fields.push(ast::Field::FieldDirective(ast::FieldDirective::Link(
                            link.clone(),
                        )));
                    }
                }
                None => (),
            }

            fields.sort_by(ast::column_order);

            insert_after_last_instance(
                fields,
                ast::is_field_directive,
                ast::Field::ColumnLines { count: 1 },
            );
        }
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
