import type { PyreDevtoolsEvent, PyreDevtoolsInstanceSnapshot, PyreDevtoolsRegistrySnapshot, PyreDevtoolsTablePage, PyreDevtoolsTablePageRequest } from './index';

export interface PyreDevtoolsRegisteredClient {
  getDevtoolsInstanceSnapshot(): Promise<PyreDevtoolsInstanceSnapshot>;
  inspectDevtoolsTablePage(request: Omit<PyreDevtoolsTablePageRequest, 'instanceId'>): Promise<PyreDevtoolsTablePage>;
  onDevtoolsEvent(callback: (event: PyreDevtoolsEvent) => void): () => void;
}

interface RegistryEntry {
  instanceId: string;
  client: PyreDevtoolsRegisteredClient;
}

const entries = new Map<string, RegistryEntry>();
const subscribers = new Set<() => void>();
let nextInstanceId = 1;

export function registerPyreDevtoolsClient(client: PyreDevtoolsRegisteredClient): string {
  const instanceId = `pyre-instance-${nextInstanceId}`;
  nextInstanceId += 1;
  entries.set(instanceId, { instanceId, client });
  notifySubscribers();
  return instanceId;
}

export function unregisterPyreDevtoolsClient(instanceId: string): void {
  if (entries.delete(instanceId)) {
    notifySubscribers();
  }
}

export function subscribePyreDevtoolsRegistry(callback: () => void): () => void {
  subscribers.add(callback);
  return () => {
    subscribers.delete(callback);
  };
}

export async function getPyreDevtoolsRegistrySnapshot(): Promise<PyreDevtoolsRegistrySnapshot> {
  const instanceSnapshots = await Promise.all(
    Array.from(entries.values()).map((entry) => entry.client.getDevtoolsInstanceSnapshot())
  );
  const labelCounts = new Map<string, number>();

  return {
    instances: instanceSnapshots.map((snapshot, index) => {
      const baseLabel = snapshot.cacheNamespace || `Instance ${index + 1}`;
      const count = (labelCounts.get(baseLabel) ?? 0) + 1;
      labelCounts.set(baseLabel, count);
      return {
        instanceId: snapshot.instanceId,
        label: count === 1 ? baseLabel : `${baseLabel} (${count})`,
        cacheNamespace: snapshot.cacheNamespace,
      };
    }),
  };
}

export async function getPyreDevtoolsInstanceSnapshot(instanceId: string): Promise<PyreDevtoolsInstanceSnapshot | null> {
  const entry = entries.get(instanceId);
  if (!entry) {
    return null;
  }
  return entry.client.getDevtoolsInstanceSnapshot();
}

export async function inspectPyreDevtoolsTablePage(request: PyreDevtoolsTablePageRequest): Promise<PyreDevtoolsTablePage> {
  const entry = entries.get(request.instanceId);
  if (!entry) {
    return { rows: [], offset: request.offset ?? 0, limit: request.limit ?? 100, hasMore: false };
  }
  return entry.client.inspectDevtoolsTablePage({
    databaseId: request.databaseId,
    tableName: request.tableName,
    offset: request.offset,
    limit: request.limit,
    filter: request.filter,
    sort: request.sort,
  });
}

export function subscribePyreDevtoolsInstanceEvents(instanceId: string, callback: (event: PyreDevtoolsEvent) => void): () => void {
  const entry = entries.get(instanceId);
  if (!entry) {
    return () => {};
  }
  return entry.client.onDevtoolsEvent(callback);
}

export function __resetPyreDevtoolsRegistryForTests(): void {
  entries.clear();
  subscribers.clear();
  nextInstanceId = 1;
}

function notifySubscribers(): void {
  subscribers.forEach((callback) => {
    callback();
  });
}
