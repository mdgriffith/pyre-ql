use crate::ast;
use std::collections::{HashMap, HashSet};

/// Collect all type definitions and sort them by dependency order
pub fn sort_types_by_dependency(database: &ast::Database) -> Vec<(String, Vec<ast::Variant>)> {
    // Collect all tagged union types
    let mut types: HashMap<String, (Vec<ast::Variant>, HashSet<String>)> = HashMap::new();

    for schema in &database.schemas {
        for file in &schema.files {
            for def in &file.definitions {
                if let ast::Definition::Tagged { name, variants, .. } = def {
                    let mut deps = HashSet::new();

                    // Collect dependencies from all variant fields
                    for variant in variants {
                        if let Some(fields) = &variant.fields {
                            for field in fields {
                                if let ast::Field::Column(col) = field {
                                    let type_str = col.type_.to_string();
                                    // Skip primitives and known types
                                    match type_str.as_str() {
                                        "String" | "Int" | "Float" | "Bool" | "DateTime"
                                        | "Json" => {}
                                        _ if type_str.starts_with("Id.Int<")
                                            || type_str.starts_with("Id.Uuid<") => {}
                                        other => {
                                            deps.insert(other.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    types.insert(name.clone(), (variants.clone(), deps));
                }
            }
        }
    }

    // Topological sort using Kahn's algorithm
    let mut sorted = Vec::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // Build graph and calculate in-degrees
    for (name, (_, deps)) in &types {
        in_degree.entry(name.clone()).or_insert(0);
        for dep in deps {
            if types.contains_key(dep) && dep != name {
                graph.entry(dep.clone()).or_default().push(name.clone());
                *in_degree.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    // Start with nodes that have no dependencies
    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(name, _)| name.clone())
        .collect();

    queue.sort(); // For deterministic output

    while let Some(name) = queue.pop() {
        if let Some((variants, _)) = types.remove(&name) {
            sorted.push((name.clone(), variants));
        }

        if let Some(dependents) = graph.get(&name) {
            for dependent in dependents {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(dependent.clone());
                    }
                }
            }
        }
    }

    // Handle any remaining types (cycles or missing deps)
    let mut remaining: Vec<(String, Vec<ast::Variant>)> = types
        .into_iter()
        .map(|(name, (variants, _))| (name, variants))
        .collect();
    remaining.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.extend(remaining);

    sorted
}

/// Generate the shared JSON type definition and schema
pub fn json_type_definition() -> &'static str {
    r#"// JSON type - represents any valid JSON value
export type JsonPrimitive = string | number | boolean | null;
export type JsonObject = { [key: string]: JsonValue };
export type JsonArray = JsonValue[];
export type JsonValue = JsonPrimitive | JsonObject | JsonArray;

export const Json: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.string(),
    z.number(),
    z.boolean(),
    z.null(),
    z.array(Json),
    z.record(z.string(), Json),
  ])
);

"#
}

/// Generate the coercion helpers
pub fn coercion_helpers() -> &'static str {
    r#"export const CoercedDate = z.number().transform((val) => new Date(val * 1000));
export const CoercedBool = z.union([z.boolean(), z.number()]).transform((val) => typeof val === 'number' ? val !== 0 : val);

"#
}

/// Generate a tagged union decoder using Zod
pub fn generate_tagged_union(name: &str, variants: &[ast::Variant]) -> String {
    let mut result = String::new();

    // Generate the Zod decoder using discriminatedUnion
    result.push_str(&format!(
        "export const {} = z.discriminatedUnion(\"type_\", [\n",
        name
    ));
    for variant in variants {
        result.push_str("  z.object({\n");
        result.push_str(&format!("    type_: z.literal(\"{}\"),\n", variant.name));

        if let Some(fields) = &variant.fields {
            for field in fields {
                if let ast::Field::Column(col) = field {
                    let type_str = col.type_.to_string();
                    let validator = match type_str.as_str() {
                        "String" => "z.string()",
                        "Int" | "Float" => "z.number()",
                        "Bool" => "CoercedBool",
                        "DateTime" => "CoercedDate",
                        "Json" => "Json",
                        _ if type_str.starts_with("Id.Int<")
                            || type_str.starts_with("Id.Uuid<") =>
                        {
                            "z.number()"
                        }
                        other => other,
                    };
                    let validator = if col.nullable {
                        format!("{}.optional()", validator)
                    } else {
                        validator.to_string()
                    };
                    result.push_str(&format!("    {}: {},\n", col.name, validator));
                }
            }
        }
        result.push_str("  }),\n");
    }
    result.push_str("]);\n\n");

    // Type inference
    result.push_str(&format!(
        "export type {} = z.infer<typeof {}>;\n\n",
        name, name
    ));

    result
}

/// Convert a type string to its Zod validator representation
pub fn type_to_zod_validator(type_str: &str, nullable: bool) -> String {
    let validator = match type_str {
        "String" => "z.string()".to_string(),
        "Int" | "Float" => "z.number()".to_string(),
        "Bool" => "CoercedBool".to_string(),
        "DateTime" => "CoercedDate".to_string(),
        "Json" => "Json".to_string(),
        _ if type_str.starts_with("Id.Int<") || type_str.starts_with("Id.Uuid<") => {
            "z.number()".to_string()
        }
        other => format!("z.any() /* {} */", other),
    };

    if nullable {
        format!("{}.optional()", validator)
    } else {
        validator
    }
}

/// Convert a type string to its TypeScript type representation
pub fn type_to_ts_type(type_str: &str) -> &str {
    match type_str {
        "String" => "string",
        "Int" | "Float" => "number",
        "Bool" => "boolean",
        "DateTime" => "Date",
        "Json" => "Json",
        _ if type_str.starts_with("Id.Int<") || type_str.starts_with("Id.Uuid<") => "number",
        other => other,
    }
}
