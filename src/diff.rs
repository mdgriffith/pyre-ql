use crate::ast;
use serde::{Deserialize, Serialize};

// Define a type to represent the diff of two schemas
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaDiff {
    pub added: Vec<crate::ast::Definition>,
    pub removed: Vec<crate::ast::Definition>,
    pub modified_records: Vec<DetailedRecordDiff>,
    pub modified_taggeds: Vec<DetailedTaggedDiff>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetailedTaggedDiff {
    pub name: String,
    pub changes: Vec<TaggedChange>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetailedRecordDiff {
    pub name: String,
    pub changes: Vec<RecordChange>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TaggedChange {
    AddedVariant(crate::ast::Variant),
    RemovedVariant(crate::ast::Variant),
    ModifiedVariant {
        name: String,
        old: crate::ast::Variant,
        new: crate::ast::Variant,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RecordChange {
    AddedField(crate::ast::Column),
    RemovedField(crate::ast::Column),
    ModifiedField { name: String, changes: ColumnDiff },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnDiff {
    pub type_changed: Option<(String, String)>, // (old_type, new_type)
    pub nullable_changed: Option<(bool, bool)>, // (old_nullable, new_nullable)
    pub added_directives: Vec<crate::ast::ColumnDirective>,
    pub removed_directives: Vec<crate::ast::ColumnDirective>,
}

// Function to diff two Schema values
pub fn diff_schema(schema1: &crate::ast::Schema, schema2: &crate::ast::Schema) -> SchemaDiff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified_records = Vec::new();
    let mut modified_taggeds = Vec::new();

    // Collect `Tagged` and `Record` definitions from both schemas
    let mut defs1: Vec<ast::Definition> = vec![];
    for file in schema1.files.iter() {
        for def in &file.definitions {
            match def {
                crate::ast::Definition::Tagged { .. } | crate::ast::Definition::Record { .. } => {
                    defs1.push(def.clone());
                }
                _ => continue,
            }
        }
    }

    let mut defs2: Vec<ast::Definition> = vec![];
    for file in schema2.files.iter() {
        for def in &file.definitions {
            match def {
                crate::ast::Definition::Tagged { .. } | crate::ast::Definition::Record { .. } => {
                    defs2.push(def.clone());
                }
                _ => continue,
            }
        }
    }

    // Find added and modified definitions
    for def2 in &defs2 {
        if let Some(def1) = defs1.iter().find(|&d| match (d, def2) {
            (
                crate::ast::Definition::Tagged { name: name1, .. },
                crate::ast::Definition::Tagged { name: name2, .. },
            ) => name1 == name2,
            (
                crate::ast::Definition::Record { name: name1, .. },
                crate::ast::Definition::Record { name: name2, .. },
            ) => name1 == name2,
            _ => false,
        }) {
            if def1 != def2 {
                match def2 {
                    crate::ast::Definition::Tagged { name, .. } => {
                        let changes = find_tagged_changes(def1, def2);
                        modified_taggeds.push(DetailedTaggedDiff {
                            name: name.clone(),
                            changes,
                        });
                    }
                    crate::ast::Definition::Record { name, .. } => {
                        let changes = find_record_changes(def1, def2);
                        modified_records.push(DetailedRecordDiff {
                            name: name.clone(),
                            changes,
                        });
                    }
                    _ => {}
                }
            }
        } else {
            added.push((*def2).clone());
        }
    }

    // Find removed definitions
    for def1 in &defs1 {
        if !defs2.iter().any(|d| match (d, def1) {
            (
                crate::ast::Definition::Tagged { name: name2, .. },
                crate::ast::Definition::Tagged { name: name1, .. },
            ) => name1 == name2,
            (
                crate::ast::Definition::Record { name: name2, .. },
                crate::ast::Definition::Record { name: name1, .. },
            ) => name1 == name2,
            _ => false,
        }) {
            removed.push((*def1).clone());
        }
    }

    SchemaDiff {
        added,
        removed,
        modified_records,
        modified_taggeds,
    }
}

// Function to find changes between two Tagged definitions
fn find_tagged_changes(
    def1: &crate::ast::Definition,
    def2: &crate::ast::Definition,
) -> Vec<TaggedChange> {
    match (def1, def2) {
        (
            crate::ast::Definition::Tagged { variants: v1, .. },
            crate::ast::Definition::Tagged { variants: v2, .. },
        ) => diff_variants(v1, v2),
        _ => vec![],
    }
}

// Function to find changes between two Record definitions
fn find_record_changes(
    def1: &crate::ast::Definition,
    def2: &crate::ast::Definition,
) -> Vec<RecordChange> {
    match (def1, def2) {
        (
            crate::ast::Definition::Record { fields: f1, .. },
            crate::ast::Definition::Record { fields: f2, .. },
        ) => diff_fields(&ast::collect_columns(f1), &ast::collect_columns(f2)),
        _ => vec![],
    }
}

// Function to find changes between two lists of fields
fn diff_fields(
    fields1: &[crate::ast::Column],
    fields2: &[crate::ast::Column],
) -> Vec<RecordChange> {
    let mut changes = Vec::new();

    for field2 in fields2 {
        if let Some(field1) = fields1.iter().find(|f| f.name == field2.name) {
            if let Some(column_diff) = diff_column(field1, field2) {
                changes.push(RecordChange::ModifiedField {
                    name: field2.name.clone(),
                    changes: column_diff,
                });
            }
        } else {
            changes.push(RecordChange::AddedField(field2.clone()));
        }
    }

    for field1 in fields1 {
        if !fields2.iter().any(|f| f.name == field1.name) {
            changes.push(RecordChange::RemovedField(field1.clone()));
        }
    }

    changes
}

// Function to find changes between two lists of variants
fn diff_variants(
    variants1: &[crate::ast::Variant],
    variants2: &[crate::ast::Variant],
) -> Vec<TaggedChange> {
    let mut changes = Vec::new();

    for variant2 in variants2 {
        if let Some(variant1) = variants1.iter().find(|v| v.name == variant2.name) {
            if variant1 != variant2 {
                changes.push(TaggedChange::ModifiedVariant {
                    name: variant2.name.clone(),
                    old: variant1.clone(),
                    new: variant2.clone(),
                });
            }
        } else {
            changes.push(TaggedChange::AddedVariant(variant2.clone()));
        }
    }

    for variant1 in variants1 {
        if !variants2.iter().any(|v| v.name == variant1.name) {
            changes.push(TaggedChange::RemovedVariant(variant1.clone()));
        }
    }

    changes
}

fn diff_column(old: &crate::ast::Column, new: &crate::ast::Column) -> Option<ColumnDiff> {
    let mut has_changes = false;
    let mut diff = ColumnDiff {
        type_changed: None,
        nullable_changed: None,
        added_directives: Vec::new(),
        removed_directives: Vec::new(),
    };

    // Create HashMaps with directive keys
    let mut old_directives = std::collections::HashMap::new();
    let mut new_directives = std::collections::HashMap::new();

    // Helper function to get directive key
    let get_key = |directive: &crate::ast::ColumnDirective| -> String {
        match directive {
            crate::ast::ColumnDirective::PrimaryKey => "_key".to_string(),
            crate::ast::ColumnDirective::Unique => "_uniq".to_string(),
            crate::ast::ColumnDirective::Default { id, .. } => id.clone(),
        }
    };

    // Populate HashMaps
    for directive in &old.directives {
        old_directives.insert(get_key(directive), directive.clone());
    }
    for directive in &new.directives {
        new_directives.insert(get_key(directive), directive.clone());
    }

    // Find added directives
    for (key, directive) in &new_directives {
        if !old_directives.contains_key(key) {
            diff.added_directives.push(directive.clone());
            has_changes = true;
        }
    }

    // Find removed directives
    for (key, directive) in &old_directives {
        if !new_directives.contains_key(key) {
            diff.removed_directives.push(directive.clone());
            has_changes = true;
        }
    }

    if has_changes {
        Some(diff)
    } else {
        None
    }
}
