// @ts-nocheck
import { expect, test } from 'bun:test';

import {
  deriveIndexedDbName,
  requireDatabaseId,
  resolveEndpointUrl,
  withDatabaseId,
} from './routing';

test('requireDatabaseId rejects missing identifiers', () => {
  expect(() => requireDatabaseId(undefined)).toThrow('databaseId is required');
  expect(() => requireDatabaseId('')).toThrow('databaseId is required');
  expect(() => requireDatabaseId('   ')).toThrow('databaseId is required');
});

test('resolveEndpointUrl preserves base paths and appends databaseId', () => {
  expect(
    resolveEndpointUrl('https://api.example.test/pyre', '/sync/events', {
      databaseId: 'campaign:123',
    })
  ).toBe('https://api.example.test/pyre/sync/events?databaseId=campaign%3A123');
});

test('withDatabaseId preserves existing query params', () => {
  expect(withDatabaseId('https://api.example.test/db?x=1', 'main')).toBe(
    'https://api.example.test/db?x=1&databaseId=main'
  );
});

test('withDatabaseId supports relative URLs', () => {
  expect(withDatabaseId('/db?x=1', 'campaign:123')).toBe('/db?x=1&databaseId=campaign%3A123');
});

test('deriveIndexedDbName is readable and includes namespace and database id', () => {
  const name = deriveIndexedDbName('pyre-client', 'user:42', 'campaign:123');

  expect(name).toStartWith('pyre-client:user_42:campaign_123_');
});

test('deriveIndexedDbName avoids collisions for similarly sanitized ids', () => {
  const colon = deriveIndexedDbName('pyre-client', 'user:42', 'campaign:123');
  const slash = deriveIndexedDbName('pyre-client', 'user:42', 'campaign/123');

  expect(colon).not.toBe(slash);
});
