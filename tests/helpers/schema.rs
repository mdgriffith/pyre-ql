/// Schema version 1: Basic records with union types
pub const SCHEMA_V1: &str = r#"
    record User {
        id   Int    @id
        name String
        status Status
    }

    type Status
       = Active
       | Inactive
       | Special {
            reason String
         }
"#;

/// Schema version 2: Adds relationships (one-to-many)
pub const SCHEMA_V2: &str = r#"
    record User {
        id   Int    @id
        name String
        status Status
        posts @link(Post.authorId)
    }

    record Post {
        id        Int    @id
        title     String
        content   String
        authorId  Int
        author    @link(authorId, User.id)
    }

    type Status
       = Active
       | Inactive
       | Special {
            reason String
         }
"#;

/// Schema version 3: Adds more records and relationships
pub const SCHEMA_V3: &str = r#"
    record User {
        id   Int    @id
        name String
        status Status
        posts @link(Post.authorId)
        accounts @link(Account.userId)
    }

    record Post {
        id        Int    @id
        title     String
        content   String
        authorId  Int
        author    @link(authorId, User.id)
    }

    record Account {
        id     Int   @id
        userId Int
        name   String
        status String
        user   @link(userId, User.id)
    }

    type Status
       = Active
       | Inactive
       | Special {
            reason String
         }
"#;

/// Get the full schema (version 3) for tests that don't need migrations
pub fn full_schema() -> String {
    SCHEMA_V3.to_string()
}
