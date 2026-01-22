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
        this.handleMessage(message as { type?: string }).catch((error) => {
          console.error('[PyreClient] Query manager handler failed:', error);
        });
      });
    }
  }

  registerQuery(registration: QueryRegistration, callback: QueryResultCallback): void {
    this.queryCallbacks.set(registration.callbackPort, callback);
    this.elmApp?.ports.receiveQueryManagerMessage?.send({
      type: 'registerQuery',
      queryId: registration.queryId,
      query: registration.query,
      input: registration.input,
      callbackPort: registration.callbackPort,
    });
  }

  updateQueryInput(queryId: string, input: unknown): void {
    this.elmApp?.ports.receiveQueryManagerMessage?.send({
      type: 'updateQueryInput',
      queryId,
      input,
    });
  }

  unregisterQuery(queryId: string, callbackPort: string): void {
    this.queryCallbacks.delete(callbackPort);
    this.elmApp?.ports.receiveQueryManagerMessage?.send({
      type: 'unregisterQuery',
      queryId,
    });
  }

  sendMutation(hash: string, baseUrl: string, input: unknown, callback?: MutationResultCallback): void {
    if (callback) {
      const callbacks = this.mutationCallbacks.get(hash) ?? [];
      callbacks.push(callback);
      this.mutationCallbacks.set(hash, callbacks);
    }

    this.elmApp?.ports.receiveQueryManagerMessage?.send({
      type: 'sendMutation',
      hash,
      baseUrl,
      input,
    });
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
