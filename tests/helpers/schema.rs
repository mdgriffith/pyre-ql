/// Status type definition
pub const STATUS_TYPE: &str = r#"
    type Status
       = Active
       | Inactive
       | Special {
            reason String
         }
"#;

/// Schema version 1: User record
pub const SCHEMA_V1: &str = r#"
    record User {
        id   Int    @id
        name String
        status Status
    }
"#;

/// Schema version 2: Post record
pub const SCHEMA_V2: &str = r#"
    record Post {
        id        Int    @id
        title     String
        content   String
        authorId  Int
        author    @link(authorId, User.id)
    }
"#;

/// Schema version 3: Account record
pub const SCHEMA_V3: &str = r#"
    record Account {
        id     Int   @id
        userId Int
        name   String
        status String
        user   @link(userId, User.id)
    }
"#;

/// Get schema version 1 as a complete schema (User + Status)
pub fn schema_v1_complete() -> String {
    format!(
        r#"
    {}

    {}
"#,
        SCHEMA_V1.trim(),
        STATUS_TYPE.trim()
    )
}

/// Get schema version 2 as a complete schema (V1 + V2 + Status)
pub fn schema_v2_complete() -> String {
    format!(
        r#"
    record User {{
        id   Int    @id
        name String
        status Status
        posts @link(Post.authorId)
    }}

    {}

    {}
"#,
        SCHEMA_V2.trim(),
        STATUS_TYPE.trim()
    )
}

/// Get schema version 3 as a complete schema (V1 + V2 + V3)
pub fn schema_v3_complete() -> String {
    full_schema()
}

/// Get the full schema (version 3) for tests that don't need migrations
pub fn full_schema() -> String {
    format!(
        r#"
    record User {{
        id   Int    @id
        name String
        status Status
        posts @link(Post.authorId)
        accounts @link(Account.userId)
    }}

    {}

    {}

    {}
"#,
        SCHEMA_V2.trim(),
        SCHEMA_V3.trim(),
        STATUS_TYPE.trim()
    )
}
