// @ts-nocheck
import { expect, test } from 'bun:test';

import { IndexedDbService, type SyncCursor } from './indexeddb';

test('IndexedDbService restores persisted sync cursor with initial data', async () => {
  const persistedCursor: SyncCursor = {
    tables: {
      maps: {
        last_seen_updated_at: null,
        permission_hash: 'perm-hash',
      },
    },
  };

  const sentMessages: unknown[] = [];
  let handleIndexedDbOut: ((message: unknown) => void | Promise<void>) | null = null;

  const storage = {
    init: async () => undefined,
    getAllTables: async () => ({ maps: [] }),
    getSyncCursor: async () => persistedCursor,
    putSyncCursor: async (cursor: SyncCursor) => {
      Object.assign(persistedCursor, cursor);
    },
    putRows: async () => undefined,
  };

  const service = new IndexedDbService(storage as never);
  service.attachPorts({
    ports: {
      indexedDbOut: {
        subscribe: (callback) => {
          handleIndexedDbOut = callback;
        },
      },
      receiveIndexedDbMessage: {
        send: (message) => {
          sentMessages.push(message);
        },
      },
    },
  });

  if (!handleIndexedDbOut) {
    throw new Error('indexedDbOut handler was not attached');
  }

  const indexedDbOut = handleIndexedDbOut;

  indexedDbOut({
    type: 'writeSyncCursor',
    cursor: persistedCursor,
  });
  await Bun.sleep(0);

  indexedDbOut({ type: 'requestInitialData' });
  await Bun.sleep(0);

  expect(sentMessages).toEqual([
    {
      type: 'initialData',
      data: {
        tables: { maps: [] },
        cursor: persistedCursor,
      },
    },
  ]);
});
