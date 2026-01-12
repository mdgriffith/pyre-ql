/**
 * Example usage of Pyre Client
 */

import { PyreClient } from '../src/index';

async function example() {
  // Initialize client
  const client = new PyreClient({
    baseUrl: 'http://localhost:3000',
    userId: 1,
    dbName: 'example-db',
    retry: {
      maxRetries: 3,
      initialDelay: 1000,
    },
  });

  // Initialize (connects WebSocket and syncs)
  await client.init((progress) => {
    if (progress.table) {
      console.log(`Syncing table: ${progress.table}`);
    }
    if (progress.complete) {
      console.log('Initial sync complete!');
    }
  });

  // Query posts with live updates
  const unsubscribePosts = client.query({
    posts: {
      id: true,
      title: true,
      content: true,
      '@where': { published: true },
      '@sort': [{ field: 'createdAt', direction: 'desc' }],
      '@limit': 10,
      users: {  // Many-to-one relationship
        id: true,
        name: true,
        email: true,
      },
    },
  }, (data) => {
    console.log('Posts updated:', data.posts);
    // Update your UI here
  });

  // Query users with their posts (one-to-many)
  const unsubscribeUsers = client.query({
    users: {
      id: true,
      name: true,
      email: true,
      posts: {  // One-to-many relationship
        id: true,
        title: true,
        '@where': { published: true },
        '@sort': [{ field: 'createdAt', direction: 'desc' }],
      },
    },
  }, (data) => {
    console.log('Users updated:', data.users);
    // Each user will have a posts array
  });

  // Complex query with filters
  const unsubscribeComplex = client.query({
    posts: {
      id: true,
      title: true,
      '@where': {
        published: true,
        createdAt: { $gte: '2024-01-01' },
        $or: [
          { title: { $in: ['Important', 'Urgent'] } },
          { content: { $ne: null } },
        ],
      },
      '@sort': [
        { field: 'createdAt', direction: 'desc' },
        { field: 'title', direction: 'asc' },
      ],
      '@limit': 20,
    },
  }, (data) => {
    console.log('Filtered posts:', data.posts);
  });

  // Monitor sync status
  const status = client.getSyncStatus();
  console.log('Sync status:', status);

  // Register sync progress callback
  const unsubscribeProgress = client.onSyncProgress((progress) => {
    console.log('Sync progress:', progress);
  });

  // Later: cleanup
  // unsubscribePosts();
  // unsubscribeUsers();
  // unsubscribeComplex();
  // unsubscribeProgress();
  // client.disconnect();
}

// Run example (commented out to avoid execution)
// example().catch(console.error);
