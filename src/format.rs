use crate::ast;
use crate::error;
use crate::typecheck;
use std::collections::HashMap;
use std::collections::HashSet;

pub fn schema(schem: &mut ast::Schema) {
    // Insert some lines before each definition if needed
    let mut i = 0;
    let mut prev_was_lines = false;

    // Insert some blank lines if needed
    for file in schem.files.iter_mut() {
        while i < file.definitions.len() {
            if let ast::Definition::Lines { .. } = file.definitions[i] {
                prev_was_lines = true;
            } else if !prev_was_lines {
                file.definitions
                    .insert(i, ast::Definition::Lines { count: 1 });
                // Move to the next element after insertion
                i += 1;
            } else {
                prev_was_lines = false;
            }
            i += 1;
        }
    }

    let mut links: HashMap<String, Vec<(bool, ast::LinkDetails)>> = HashMap::new();
    // Get all links and calculate reciprocals
    for file in schem.files.iter() {
        for def in &file.definitions {
            if let ast::Definition::Record {
                name,
                fields,
                start,
                end,
                start_name,
                end_name,
            } = def
            {
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
    }

    // Standard formatting
    for file in schem.files.iter_mut() {
        for definition in &mut file.definitions {
            format_definition(definition, &links);
        }
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
        ast::Definition::Session(_) => (),
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
            let empty_links = &vec![];
            let links_on_this_table = links.get(name).unwrap_or(empty_links);
            let mut links_missing: Vec<ast::LinkDetails> = vec![];
            let mut links_to_remove: Vec<ast::LinkDetails> = vec![];

            // See if any calculated links are missing
            for (is_calculated, link) in links_on_this_table {
                let mut found = false;
                for field in fields.iter() {
                    match field {
                        ast::Field::FieldDirective(ast::FieldDirective::Link(existing_link)) => {
                            if existing_link.link_name == link.link_name {
                                found = true;
                            }
                        }
                        _ => (),
                    }
                }
                if !found {
                    links_missing.push(link.clone());
                }
            }

            // See if there are links that should be removed
            for field in fields.iter() {
                match field {
                    ast::Field::FieldDirective(ast::FieldDirective::Link(existing_link)) => {
                        let mut found = false;
                        for (is_calculated, link) in links_on_this_table {
                            if link.link_name == existing_link.link_name {
                                found = true;
                            }
                        }
                        if !found {
                            links_to_remove.push(existing_link.clone());
                        }
                    }
                    _ => (),
                }
            }

            let mut previous_count: Option<usize> = None;
            let mut removed_fields = HashSet::new();
            let mut i = 0;

            // Merge adjacent ColumnLines items and have a max count of 2
            for field in fields.iter_mut() {
                match field {
                    ast::Field::ColumnLines { count } => {
                        if *count == 0 {
                            removed_fields.insert(i);
                        } else {
                            match previous_count {
                                Some(prev_count) => {
                                    *count += prev_count;
                                    *count = (*count).min(2);
                                    removed_fields.insert(i - 1);
                                }
                                None => {
                                    previous_count = Some(*count);
                                }
                            }
                        }
                    }

                    _ => {
                        previous_count = None;
                    }
                }
                i += 1;
            }

            // Remove the fields that were marked for removal
            let mut i = 0;
            fields.retain(|_| {
                let should_keep = !removed_fields.contains(&i);
                i += 1;
                should_keep
            });

            // Remove unnecessary links
            fields.retain(|field| {
                if let ast::Field::FieldDirective(ast::FieldDirective::Link(link)) = field {
                    !links_to_remove.contains(link)
                } else {
                    true
                }
            });

            // Add missing links
            for link in links_missing.drain(..) {
                fields.push(ast::Field::FieldDirective(ast::FieldDirective::Link(link)));
            }
        }
    }
}

/* Queries

The main thing that query_list does is calculate what the inferred param types are for each query

*/
pub fn query_list(schem: &ast::Schema, queries: &mut ast::QueryList) {
    match typecheck::populate_context(schem) {
        Ok(context) => {
            let mut query_param_map = HashMap::new();

            for query in queries.queries.iter() {
                match query {
                    ast::QueryDef::Query(q) => {
                        let mut errors: Vec<error::Error> = Vec::new();
                        let params = typecheck::check_query(&context, &mut errors, q);
                        query_param_map.insert(q.name.clone(), params);
                    }
                    _ => (),
                }
            }

            for query in queries.queries.iter_mut() {
                match query {
                    ast::QueryDef::Query(ref mut q) => match query_param_map.get_mut(&q.name) {
                        Some(calculated_params) => {
                            for arg in q.args.iter_mut() {
                                // Add $ to the front of the arg name
                                let param_name = format!("${}", arg.name);

                                //
                                match calculated_params.get(&param_name) {
                                    Some(param) => {
                                        match param {
                                            typecheck::ParamInfo::Defined { type_, .. } => {
                                                arg.type_ = type_.clone();
                                            }
                                            typecheck::ParamInfo::NotDefinedButUsed {
                                                used_at,
                                                type_,
                                            } => {
                                                arg.type_ = type_.clone();
                                            }
                                        }

                                        calculated_params.remove(&param_name);
                                    }
                                    None => (),
                                }
                            }

                            for (name, param) in calculated_params.iter() {
                                match param {
                                    typecheck::ParamInfo::Defined { .. } => (),
                                    typecheck::ParamInfo::NotDefinedButUsed { used_at, type_ } => {
                                        q.args.push(ast::QueryParamDefinition {
                                            name: name.clone(),
                                            type_: type_.clone(),
                                            start_name: None,
                                            end_name: None,
                                            start_type: None,
                                            end_type: None,
                                        });
                                    }
                                }
                            }
                        }
                        None => (),
                    },
                    _ => (),
                }
            }
        }
        Err(errors) => (),
    }
}
