import type { ElmApp } from '../types';

export interface QueryRegistration {
  queryId: string;
  query: unknown;
  input: unknown;
  callbackPort: string;
}

export interface MutationResult {
  ok: boolean;
  value?: unknown;
  error?: string;
}

type QueryResultCallback = (result: unknown) => void;
type MutationResultCallback = (result: MutationResult) => void;

export class QueryManagerService {
  private elmApp: ElmApp | null = null;
  private queryCallbacks: Map<string, QueryResultCallback> = new Map();
  private mutationCallbacks: Map<string, MutationResultCallback[]> = new Map();

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;

    if (elmApp.ports.queryManagerOut) {
      elmApp.ports.queryManagerOut.subscribe((message) => {
        console.log('[PyreClient] port queryManagerOut <-', message);
        this.handleMessage(message as { type?: string }).catch((error) => {
          console.error('[PyreClient] Query manager handler failed:', error);
        });
      });
    }
  }

  registerQuery(registration: QueryRegistration, callback: QueryResultCallback): void {
    this.queryCallbacks.set(registration.callbackPort, callback);
    const registerMessage = {
      type: 'registerQuery',
      queryId: registration.queryId,
      query: registration.query,
      input: registration.input,
      callbackPort: registration.callbackPort,
    };
    this.elmApp?.ports.receiveQueryManagerMessage?.send(registerMessage);
    console.log('[PyreClient] port receiveQueryManagerMessage ->', registerMessage);
  }

  updateQueryInput(queryId: string, input: unknown, query?: unknown): void {
    const updateMessage = {
      type: 'updateQueryInput',
      queryId,
      query,
      input,
    };
    this.elmApp?.ports.receiveQueryManagerMessage?.send(updateMessage);
    console.log('[PyreClient] port receiveQueryManagerMessage ->', updateMessage);
  }

  unregisterQuery(queryId: string, callbackPort: string): void {
    this.queryCallbacks.delete(callbackPort);
    const unregisterMessage = {
      type: 'unregisterQuery',
      queryId,
    };
    this.elmApp?.ports.receiveQueryManagerMessage?.send(unregisterMessage);
    console.log('[PyreClient] port receiveQueryManagerMessage ->', unregisterMessage);
  }

  sendMutation(hash: string, baseUrl: string, input: unknown, callback?: MutationResultCallback): void {
    if (callback) {
      const callbacks = this.mutationCallbacks.get(hash) ?? [];
      callbacks.push(callback);
      this.mutationCallbacks.set(hash, callbacks);
    }

    const mutationMessage = {
      type: 'sendMutation',
      hash,
      baseUrl,
      input,
    };
    this.elmApp?.ports.receiveQueryManagerMessage?.send(mutationMessage);
    console.log('[PyreClient] port receiveQueryManagerMessage ->', mutationMessage);
  }

  private async handleMessage(message: { type?: string }): Promise<void> {
    if (message.type === 'queryResult') {
      const typedMessage = message as { callbackPort?: string; result?: unknown };
      if (!typedMessage.callbackPort) {
        return;
      }
      const callback = this.queryCallbacks.get(typedMessage.callbackPort);
      if (callback) {
        callback(typedMessage.result);
      }
      return;
    }

    if (message.type === 'mutationResult') {
      const typedMessage = message as { hash?: string; result?: MutationResult };
      if (!typedMessage.hash) {
        return;
      }
      const callbacks = this.mutationCallbacks.get(typedMessage.hash);
      if (!callbacks || callbacks.length === 0) {
        return;
      }
      const callback = callbacks.shift();
      if (callback) {
        callback(typedMessage.result ?? { ok: false, error: 'Missing mutation result' });
      }
      if (callbacks.length === 0) {
        this.mutationCallbacks.delete(typedMessage.hash);
      } else {
        this.mutationCallbacks.set(typedMessage.hash, callbacks);
      }
    }
  }
}
