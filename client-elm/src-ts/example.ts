/**
 * Example usage of Pyre Elm Client
 */

import { initPyreElmClient } from './index';
import type { SchemaMetadata } from '../client/src/types';

// Assuming you have compiled Elm and imported it
// const Elm = require('./dist/elm.js');

interface SSEConfig {
  baseUrl: string;
  userId: number;
}

interface Flags {
  schema: SchemaMetadata;
  sseConfig: SSEConfig;
}

declare const Elm: {
  Main: {
    init: (config: { flags: Flags }) => {
      ports: {
        [key: string]: {
          subscribe?: (callback: (value: any) => void) => void;
          send?: (value: any) => void;
        };
      };
    };
  };
};

export function initExample(schemaMetadata: SchemaMetadata, sseConfig: SSEConfig) {
  // Initialize Elm app with schema metadata and SSE config as flags
  const elmApp = Elm.Main.init({
    flags: {
      schema: schemaMetadata,
      sseConfig: sseConfig
    },
  });

  // Initialize TypeScript bridge
  initPyreElmClient(elmApp, 'pyre-client');

  // Handle QueryManager service messages (query results and mutation results)
  if (elmApp.ports.queryManagerOut) {
    elmApp.ports.queryManagerOut.subscribe((message: any) => {
      if (message.type === 'queryResult') {
        console.log('Query result for port:', message.callbackPort, message.result);
        // Route to appropriate callback based on callbackPort
      } else if (message.type === 'mutationResult') {
        console.log('Mutation result for hash:', message.hash, message.result);
        // Handle mutation result
      }
    });
  }

  // Register a query example
  function registerQueryExample(queryId: string, queryShape: any, input: any, callbackPort: string) {
    if (elmApp.ports.receiveQueryManagerMessage) {
      elmApp.ports.receiveQueryManagerMessage.send({
        type: 'registerQuery',
        queryId: queryId,
        queryShape: queryShape,
        input: input,
        callbackPort: callbackPort
      });
    }
  }

  // Update query input example
  function updateQueryInputExample(queryId: string, newInput: any) {
    if (elmApp.ports.receiveQueryManagerMessage) {
      elmApp.ports.receiveQueryManagerMessage.send({
        type: 'updateQueryInput',
        queryId: queryId,
        input: newInput
      });
    }
  }

  // Send mutation example
  function sendMutationExample(hash: string, baseUrl: string, input: any) {
    if (elmApp.ports.receiveQueryManagerMessage) {
      elmApp.ports.receiveQueryManagerMessage.send({
        type: 'sendMutation',
        hash: hash,
        baseUrl: baseUrl,
        input: input
      });
    }
  }

  return {
    registerQuery: registerQueryExample,
    updateQueryInput: updateQueryInputExample,
    sendMutation: sendMutationExample,
  };
}
