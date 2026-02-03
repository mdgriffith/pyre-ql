import { createDb } from '../pyre/generated/simple/db';
import {
  GetAllUsers,
  GetUser,
  GetUserPosts,
  CreateUser,
  CreatePost,
  UpdateUser,
  DeleteUser,
  type Session,
} from '../pyre/generated/simple/index';

// Create an in-memory database for demo
const db = createDb({
  url: 'file:./db/app.db'
});

// Define a session (used for permission checking)
// In a real app, this would come from authentication
const adminSession: Session = {
  userId: 1,
  role: 'admin'
};

const userSession: Session = {
  userId: 2,
  role: 'user'
};

async function runDemo() {
  console.log('=== Pyre Simple Playground Demo ===\n');

  // Initialize database with schema
  console.log('Note: In a real app, you would run migrations first:');
  console.log('  pyre migrate db/app.db\n');

  // Create some users
  console.log('Creating users...');

  await CreateUser(db, adminSession, {
    name: 'Alice',
    email: 'alice@example.com'
  });
  console.log('✓ Created Alice');

  await CreateUser(db, adminSession, {
    name: 'Bob',
    email: 'bob@example.com'
  });
  console.log('✓ Created Bob');

  // Get all users
  console.log('\nGetting all users...');
  const allUsers = await GetAllUsers(db, adminSession);
  console.log(allUsers);
  console.log(`Found ${allUsers.user.length} users:`);
  for (const user of allUsers.user) {
    console.log(`  - ${user.name} (${user.email})`);
  }

  // Get a specific user
  console.log('\nGetting user by ID...');
  const userResult = await GetUser(db, adminSession, { id: 1 });
  if (userResult.user.length > 0) {
    const user = userResult.user[0];
    console.log(`Found: ${user.name} <${user.email}>`);
    console.log(`Created at: ${user.createdAt}`);
  }

  // Update a user
  console.log('\nUpdating user...');
  await UpdateUser(db, adminSession, {
    id: 1,
    name: 'Alice Smith',
    email: 'alice.smith@example.com'
  });
  console.log('✓ Updated Alice');

  // Create posts
  console.log('\nCreating posts...');

  await CreatePost(db, adminSession, {
    title: 'Hello World',
    content: 'My first post!',
    published: true
  });
  console.log('✓ Created public post for Alice');

  await CreatePost(db, adminSession, {
    title: 'Draft Post',
    content: 'Work in progress...',
    published: false
  });
  console.log('✓ Created draft post for Alice');

  // Get posts for a user
  console.log('\nGetting posts for user 1 (as admin)...');
  const posts = await GetUserPosts(db, adminSession, { userId: 1 });
  console.log(`Found ${posts.post.length} posts:`);
  for (const post of posts.post) {
    const status = post.published ? 'published' : 'draft';
    console.log(`  - "${post.title}" (${status})`);
  }

  // Demonstrate permissions
  console.log('\n--- Permission Demo ---');
  console.log('User 2 trying to see user 1\'s posts (should only see published)...');
  const user2Posts = await GetUserPosts(db, userSession, { userId: 1 });
  console.log(`User 2 can see ${user2Posts.post.length} posts:`);
  for (const post of user2Posts.post) {
    console.log(`  - "${post.title}" (published: ${post.published})`);
  }

  // Clean up
  console.log('\n--- Cleanup ---');
  await DeleteUser(db, adminSession, { id: 2 });
  console.log('✓ Deleted Bob');

  console.log('\n=== Demo Complete ===');
}

// Run the demo
runDemo().catch(err => {
  console.error('Error:', err);
  process.exit(1);
});
