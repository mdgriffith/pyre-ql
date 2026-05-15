// @ts-nocheck
import { expect, test } from 'bun:test';

import { buildSSEUrl } from './sse';
import { buildWebSocketUrl } from './websocket';

test('buildSSEUrl includes databaseId and preserves base path', () => {
  expect(buildSSEUrl({
    baseUrl: 'https://api.example.test/pyre',
    eventsPath: '/sync/events',
    databaseId: 'campaign:123',
  })).toBe('https://api.example.test/pyre/sync/events?databaseId=campaign%3A123');
});

test('buildWebSocketUrl includes databaseId and switches protocol', () => {
  expect(buildWebSocketUrl({
    baseUrl: 'https://api.example.test/pyre',
    eventsPath: '/sync/events',
    databaseId: 'campaign:123',
  })).toBe('wss://api.example.test/pyre/sync/events?databaseId=campaign%3A123');
});
