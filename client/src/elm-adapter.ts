/**
 * Elm Port Adapter for Pyre Client
 * 
 * This module bridges Elm ports to the PyreClient IndexedDB client.
 * It listens to Elm ports for queries and mutations, executes them via PyreClient,
 * and sends results back through subscription ports.
 * 
 * Usage:
 * ```typescript
 * import { PyreClient } from '@pyre/client';
 * import { registerQueries, cleanup } from '@pyre/client/elm-adapter';
 * import { queries } from './generated/client/node/queries';
 * 
 * const client = new PyreClient({ ... });
 * await client.init();
 * 
 * // Register queries with Elm app
 * const elmApp = Elm.Main.init({ ... });
 * registerQueries(elmApp, client, queries);
 * 
 * // Cleanup when done (e.g., on app unmount)
 * cleanup();
 * ```
 */

import { PyreClient } from './index';
import type { QuerySubscription, Query } from './types';

// Type definitions for Elm port communication
// Outgoing ports (Elm -> JS): use subscribe() to listen
// Incoming ports (JS -> Elm): use send() to send data
interface ElmPort {
  subscribe?: (callback: (value: any) => void) => void;
  send?: (value: any) => void;
}

interface ElmApp {
  ports: {
    [key: string]: ElmPort;
  };
}

export type QueryModule = Query;

// Store subscription objects and port subscriptions for cleanup
interface QueryRegistration {
  subscription?: QuerySubscription<any>;
  portSubscription?: () => void;
}

const queryRegistrations = new Map<string, QueryRegistration>();

/**
 * Register a query module with the Elm adapter
 */
export function registerQuery(
  elmApp: ElmApp,
  client: PyreClient,
  queryName: string,
  queryModule: QueryModule
): void {
  // Clean up previous registration if it exists
  unregisterQuery(queryName);

  // Unified port names for all operations: pyre_send{Name} and pyre_receive{Name}
  const sendPortName = `pyre_send${queryName}`;
  const resultsPortName = `pyre_receive${queryName}`;

  const fromElm = elmApp.ports[sendPortName];
  const toElm = elmApp.ports[resultsPortName];

  const hasFromElm = fromElm && fromElm.subscribe;
  const hasToElm = toElm && toElm.send;

  // Case 1: Both ports don't exist - query is unused
  if (!hasFromElm && !hasToElm) {
    console.log(`pyre: ${queryName} is currently unused by the Elm App`);
    return;
  }

  // Case 2: fromElm missing but toElm exists - subscribed but can't start
  if (!hasFromElm && hasToElm) {
    console.warn(`pyre: You're subscribed to ${queryName}, but no part of the code starts it`);
    return;
  }

  // Case 3: Query without subscription port - warn about missing subscription
  if (queryModule.operation === 'query' && !hasToElm) {
    console.warn(`pyre: Query ${queryName} is not being subscribed to.`);
    return;
  }

  // Extract send function - use noop if toElm doesn't exist (for mutations - Case 4)
  const sendToElm = hasToElm ? toElm.send! : () => { };

  // Create registration object to store subscription
  const registration: QueryRegistration = {
    portSubscription: undefined,
  };

  // Subscribe to fromElm port - this fires whenever Elm calls send()
  // fromElm is guaranteed to exist here due to earlier checks (hasFromElm check)
  const portUnsubscribe = fromElm!.subscribe!((input: any) => {
    if (queryModule.operation === 'query') {
      // For queries, use update() if subscription exists, otherwise create new one
      if (registration.subscription) {
        // Update existing subscription with new input
        registration.subscription.update(input);
      } else {
        // Create new subscription
        const subscription = client.run(
          queryModule,
          input,
          sendToElm
        );
        if (subscription) {
          registration.subscription = subscription;
        }
      }
    } else {
      client.run(queryModule, input, sendToElm);
    }
  });

  // Store port subscription
  registration.portSubscription = portUnsubscribe as (() => void) | undefined;

  // Store registration for cleanup
  queryRegistrations.set(queryName, registration);
}

/**
 * Register multiple query modules at once
 */
export function registerQueries(
  elmApp: ElmApp,
  client: PyreClient,
  queries: Record<string, QueryModule>
): void {
  for (const [queryName, queryModule] of Object.entries(queries)) {
    registerQuery(elmApp, client, queryName, queryModule);
  }
}

/**
 * Unregister a single query
 */
export function unregisterQuery(queryName: string): void {
  const registration = queryRegistrations.get(queryName);
  if (registration) {
    if (registration.subscription) {
      registration.subscription.unsubscribe();
    }
    if (registration.portSubscription) {
      registration.portSubscription();
    }
    queryRegistrations.delete(queryName);
  }
}

/**
 * Clean up all registered queries and subscriptions
 * Call this when the Elm app is destroyed or ports disconnect
 */
export function cleanup(): void {
  for (const [queryName, registration] of queryRegistrations.entries()) {
    try {
      if (registration.subscription) {
        registration.subscription.unsubscribe();
      }
      if (registration.portSubscription) {
        registration.portSubscription();
      }
    } catch (error) {
      console.error(`[ElmAdapter] Error cleaning up query ${queryName}:`, error);
    }
  }
  queryRegistrations.clear();
}
