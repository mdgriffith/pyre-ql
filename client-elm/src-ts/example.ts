/**
 * Example usage of Pyre Elm Client
 */

import { PyreClient } from './index';
import type { SchemaMetadata } from '../../client/src/types';
import type { SSEConfig } from './types';

export function initExample(schemaMetadata: SchemaMetadata, sseConfig: SSEConfig) {
  const client = new PyreClient({
    schema: schemaMetadata,
    sseConfig,
    dbName: 'pyre-client',
  });

  client.onSyncProgress((progress) => {
    console.log('Sync progress:', progress);
  });

  return client;
}
