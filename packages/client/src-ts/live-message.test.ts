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

function deltaMessage(databaseId?: string) {
  return {
    type: 'delta',
    databaseId,
    data: [
      {
        table_name: 'maps',
        headers: ['id', 'name', 'updatedAt'],
        rows: [[1, 'World', 10]],
      },
    ],
  };
}

async function startSyncedElmApp() {
  const previousXmlHttpRequest = globalThis.XMLHttpRequest;
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  let currentMethod = 'GET';

  class MockXMLHttpRequest {
    listeners: Record<string, Array<() => void>> = {};
    status = 200;
    statusText = 'OK';
    responseURL = '';
    responseType = '';
    response = JSON.stringify({ databaseId: 'campaign:123', tables: {}, has_more: false });
    timeout = 0;
    withCredentials = false;

    addEventListener(type: string, callback: () => void) {
      this.listeners[type] = this.listeners[type] ?? [];
      this.listeners[type].push(callback);
    }

    open(method: string, url: string) {
      currentMethod = method;
      this.responseURL = url;
    }

    setRequestHeader() {}

    send(body?: string) {
      requests.push({ method: currentMethod, url: this.responseURL, body });
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

  const Elm = loadElm(Object.create(globalThis));
  const app = Elm.Main.init({
    flags: {
      schema,
      server: {
        baseUrl: 'http://example.test',
        catchupPath: '/sync',
        databaseId: 'campaign:123',
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
        cursor: { tables: {} },
      },
    });
  });

  await Bun.sleep(0);
  await Bun.sleep(0);

  return { app, requests, restore: () => { globalThis.XMLHttpRequest = previousXmlHttpRequest; } };
}

test('Elm live sync rejects missing delta databaseId when configured', async () => {
  const errors: string[] = [];
  const writes: unknown[] = [];
  const { app, restore } = await startSyncedElmApp();

  try {
    app.ports.errorOut.subscribe((message) => {
      errors.push(message);
    });
    app.ports.indexedDbOut.subscribe((message) => {
      if (message?.type === 'writeDelta') {
        writes.push(message);
      }
    });

    app.ports.receiveSSEMessage.send(deltaMessage());
    await Bun.sleep(0);

    expect(errors).toContain('Live sync delta missing databaseId: expected campaign:123');
    expect(writes).toHaveLength(0);
  } finally {
    restore();
  }
});

test('Elm live sync rejects mismatched delta databaseId before writing cache', async () => {
  const errors: string[] = [];
  const writes: unknown[] = [];
  const { app, restore } = await startSyncedElmApp();

  try {
    app.ports.errorOut.subscribe((message) => {
      errors.push(message);
    });
    app.ports.indexedDbOut.subscribe((message) => {
      if (message?.type === 'writeDelta') {
        writes.push(message);
      }
    });

    app.ports.receiveSSEMessage.send(deltaMessage('campaign:456'));
    await Bun.sleep(0);

    expect(errors).toContain('Live sync delta databaseId mismatch: expected campaign:123, got campaign:456');
    expect(writes).toHaveLength(0);
  } finally {
    restore();
  }
});

test('Elm live sync accepts matching delta databaseId', async () => {
  const errors: string[] = [];
  const writes: unknown[] = [];
  const { app, restore } = await startSyncedElmApp();

  try {
    app.ports.errorOut.subscribe((message) => {
      errors.push(message);
    });
    app.ports.indexedDbOut.subscribe((message) => {
      if (message?.type === 'writeDelta') {
        writes.push(message);
      }
    });

    app.ports.receiveSSEMessage.send(deltaMessage('campaign:123'));
    await Bun.sleep(0);

    expect(errors).toHaveLength(0);
    expect(writes).toHaveLength(1);
  } finally {
    restore();
  }
});

test('Elm live syncRequired starts catchup from the current cursor', async () => {
  const { app, requests, restore } = await startSyncedElmApp();

  try {
    const requestCountAfterInitialCatchup = requests.length;

    app.ports.receiveSSEMessage.send({
      type: 'syncRequired',
      databaseId: 'campaign:123',
    });
    await Bun.sleep(0);
    await Bun.sleep(0);

    expect(requests).toHaveLength(requestCountAfterInitialCatchup + 1);
    expect(requests.at(-1)?.method).toBe('POST');
    expect(JSON.parse(requests.at(-1)?.body ?? '{}')).toEqual({
      databaseId: 'campaign:123',
      syncCursor: {
        tables: {
          maps: {
            last_seen_updated_at: null,
            permission_hash: '',
          },
        },
      },
    });
  } finally {
    restore();
  }
});
