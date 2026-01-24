/**
 * Example usage of Pyre Elm Client
 */

import { PyreClient } from './index';
import type { SchemaMetadata } from '../../client/src/types';
import type { ServerConfig } from './types';

export function initExample(schemaMetadata: SchemaMetadata, server: ServerConfig) {
  const client = new PyreClient({
    schema: schemaMetadata,
    server,
    indexedDbName: 'pyre-client',
  });

  client.onSyncProgress((progress) => {
    console.log('Sync progress:', progress);
  });

  return client;
}
