#[path = "../helpers/mod.rs"]
mod helpers;

use helpers::{schema, TestDatabase, TestError};

#[tokio::test]
async fn test_query_with_one_to_many() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetUserWithPosts {
            user {
                id
                name
                posts {
                    id
                    title
                    content
                }
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("user"),
        "Results should contain 'user' field"
    );
    let users = results.get("user").unwrap();

    // Find user with posts
    let user_with_posts = users.iter().find(|u| {
        u.get("posts")
            .and_then(|p| p.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    });

    assert!(
        user_with_posts.is_some(),
        "Should have at least one user with posts"
    );

    let posts = user_with_posts
        .unwrap()
        .get("posts")
        .and_then(|p| p.as_array())
        .unwrap();
    assert!(posts.len() >= 2, "User should have at least 2 posts");

    Ok(())
}

#[tokio::test]
async fn test_query_with_many_to_one() -> Result<(), TestError> {
    let db = TestDatabase::new(&schema::full_schema()).await?;
    db.seed_standard_data().await?;

    let query = r#"
        query GetPostsWithAuthor {
            post {
                id
                title
                author {
                    id
                    name
                }
            }
        }
    "#;

    let rows = db.execute_query(query).await?;
    let results = db.parse_query_results(rows).await?;

    assert!(
        results.contains_key("post"),
        "Results should contain 'post' field"
    );
    let posts = results.get("post").unwrap();
    assert!(posts.len() >= 2, "Should have at least 2 posts");

    // Verify all posts have author
    for post in posts {
        assert!(
            post.get("author").is_some(),
            "Post should have author field"
        );
        let author = post.get("author").unwrap();
        assert!(author.get("id").is_some(), "Author should have id");
        assert!(author.get("name").is_some(), "Author should have name");
    }

    Ok(())
}

