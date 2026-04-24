// @ts-nocheck
import { expect, test } from 'bun:test';

import loadElm from '../dist/engine.mjs';

const schema = {
  tables: {
    maps: {
      name: 'maps',
      links: {},
      indices: [],
    },
  },
  queryFieldToTable: {},
};

test('Elm catchup request includes restored syncCursor on startup', async () => {
  const requestedUrls: string[] = [];
  const previousXmlHttpRequest = globalThis.XMLHttpRequest;

  class MockXMLHttpRequest {
    listeners: Record<string, Array<() => void>> = {};
    status = 200;
    statusText = 'OK';
    responseURL = '';
    responseType = '';
    response = JSON.stringify({ tables: {}, has_more: false });
    timeout = 0;
    withCredentials = false;
    c = false;

    addEventListener(type: string, callback: () => void) {
      this.listeners[type] = this.listeners[type] ?? [];
      this.listeners[type].push(callback);
    }

    open(_method: string, url: string) {
      this.responseURL = url;
      requestedUrls.push(url);
    }

    setRequestHeader() {}

    send() {
      queueMicrotask(() => {
        (this.listeners.load ?? []).forEach((listener) => listener());
      });
    }

    abort() {}

    getAllResponseHeaders() {
      return '';
    }
  }

  globalThis.XMLHttpRequest = MockXMLHttpRequest;

  try {
    const Elm = loadElm(Object.create(globalThis));
    const app = Elm.Main.init({
      flags: {
        schema,
        server: {
          baseUrl: 'http://example.test',
          catchupPath: '/sync',
        },
        liveSync: {
          transport: 'sse',
        },
      },
    });

    app.ports.indexedDbOut.subscribe((message) => {
      if (message?.type !== 'requestInitialData') {
        return;
      }

      app.ports.receiveIndexedDbMessage.send({
        type: 'initialData',
        data: {
          tables: { maps: [] },
          cursor: {
            tables: {
              maps: {
                last_seen_updated_at: null,
                permission_hash: 'perm-hash',
              },
            },
          },
        },
      });
    });

    await Bun.sleep(0);
    await Bun.sleep(0);

    expect(requestedUrls).toHaveLength(1);
    expect(requestedUrls[0]).toContain('/sync?syncCursor=');

    const url = new URL(requestedUrls[0]);
    expect(url.searchParams.get('syncCursor')).toBe(
      '{"tables":{"maps":{"last_seen_updated_at":null,"permission_hash":"perm-hash"}}}'
    );
  } finally {
    globalThis.XMLHttpRequest = previousXmlHttpRequest;
  }
});
