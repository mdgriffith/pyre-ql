import type { ElmApp } from '../types';

export interface QueryRegistration {
  queryId: string;
  querySource: unknown;
  input: unknown;
}

type QueryResultCallback = (result: unknown) => void;

export type QueryDeltaEnvelope =
  | {
    type: 'full';
    queryId: string;
    revision: number;
    result: unknown;
  }
  | {
    type: 'delta';
    queryId: string;
    revision: number;
    delta: { ops: QueryDeltaOp[] };
  };

export type QueryDeltaOp =
  | { op: 'set-row'; path: string; row: unknown }
  | { op: 'remove-row'; path: string }
  | { op: 'insert-row'; path: string; index: number; row: unknown }
  | { op: 'move-row'; path: string; from: number; to: number }
  | { op: 'remove-row-by-index'; path: string; index: number };

type QueryState = {
  input: unknown;
  result: unknown;
  revision: number;
  callback: QueryResultCallback;
};

type ErrorPayload = {
  message: string;
  queryId?: string;
  op?: string;
  path?: string;
  details?: string;
};

type PathSegment =
  | { type: 'field'; name: string }
  | { type: 'id'; id: string }
  | { type: 'index'; index: number };

type UpdateResult = { ok: true; node: unknown } | { ok: false; details: string };

type UpdateFn = (node: unknown) => UpdateResult;

export class QueryClientService {
  private elmApp: ElmApp | null = null;
  private queryStates: Map<string, QueryState> = new Map();
  private hasPorts = false;
  private logger: (payload: ErrorPayload) => void;

  constructor(logger?: (payload: ErrorPayload) => void) {
    this.logger = logger ?? ((payload) => {
      console.error('[PyreClient] QueryClient error', payload);
    });
  }

  attachPorts(elmApp: ElmApp): void {
    this.elmApp = elmApp;
    this.hasPorts = Boolean(
      elmApp.ports.queryClientOut && elmApp.ports.receiveQueryClientMessage
    );

    if (elmApp.ports.queryClientOut) {
      elmApp.ports.queryClientOut.subscribe((message) => {
        this.handleMessage(message).catch((error) => {
          this.logError({
            message: 'Failed to handle QueryDelta message',
            details: error instanceof Error ? error.message : String(error),
          });
        });
      });
    }
  }

  isAvailable(): boolean {
    return this.hasPorts;
  }

  registerQuery(registration: QueryRegistration, callback: QueryResultCallback): void {
    console.log('[QueryClient] registerQuery:', registration.queryId);

    this.queryStates.set(registration.queryId, {
      input: registration.input,
      result: null,
      revision: -1,
      callback,
    });

    const registerMessage = {
      type: 'register',
      queryId: registration.queryId,
      querySource: registration.querySource,
      queryInput: registration.input,
    };

    console.log('[QueryClient] sending register message to Elm:', registerMessage);
    this.elmApp?.ports.receiveQueryClientMessage?.send(registerMessage);
  }

  updateQueryInput(queryId: string, input: unknown): void {
    const state = this.queryStates.get(queryId);
    if (state) {
      state.input = input;
    }

    const updateMessage = {
      type: 'update-input',
      queryId,
      queryInput: input,
    };

    this.elmApp?.ports.receiveQueryClientMessage?.send(updateMessage);
  }

  unregisterQuery(queryId: string): void {
    console.log('[QueryClient] unregisterQuery:', queryId);
    this.queryStates.delete(queryId);

    const unregisterMessage = {
      type: 'unregister',
      queryId,
    };

    this.elmApp?.ports.receiveQueryClientMessage?.send(unregisterMessage);
  }

  private async handleMessage(message: unknown): Promise<void> {
    console.log('[QueryClient] handleMessage received:', message);

    if (!message || typeof message !== 'object') {
      console.log('[QueryClient] handleMessage: message is not an object');
      return;
    }

    const envelope = message as QueryDeltaEnvelope;
    if (envelope.type !== 'full' && envelope.type !== 'delta') {
      console.log('[QueryClient] handleMessage: unknown type', (message as any).type);
      return;
    }

    const state = this.queryStates.get(envelope.queryId);
    console.log('[QueryClient] handleMessage: queryId =', envelope.queryId, 'state =', state ? 'found' : 'NOT FOUND');
    console.log('[QueryClient] handleMessage: registered queryIds =', Array.from(this.queryStates.keys()));

    if (!state) {
      this.logError({
        message: 'QueryDelta received for unknown query',
        queryId: envelope.queryId,
      });
      return;
    }

    if (envelope.revision <= state.revision) {
      this.logError({
        message: 'QueryDelta revision out of order',
        queryId: envelope.queryId,
        details: `received=${envelope.revision} current=${state.revision}`,
      });
      return;
    }

    if (envelope.type === 'full') {
      console.log('[QueryClient] handleMessage: full result, calling callback with:', envelope.result);
      state.result = envelope.result;
      state.revision = envelope.revision;
      state.callback(state.result);
      console.log('[QueryClient] handleMessage: callback completed');
      return;
    }

    const nextResult = this.applyDelta(envelope.queryId, state.result, envelope.delta);
    state.result = nextResult;
    state.revision = envelope.revision;
    state.callback(state.result);
  }

  private applyDelta(queryId: string, result: unknown, delta: { ops: QueryDeltaOp[] }): unknown {
    let next = result;
    for (const op of delta.ops) {
      const updated = this.applyOp(queryId, next, op);
      if (updated.ok) {
        next = updated.node;
      }
    }
    return next;
  }

  private applyOp(queryId: string, result: unknown, op: QueryDeltaOp): UpdateResult {
    const parsed = parsePath(op.path);
    if (!parsed.ok) {
      this.logError({
        message: 'QueryDelta op failed: invalid path',
        queryId,
        op: op.op,
        path: op.path,
        details: parsed.details,
      });
      return { ok: false, details: parsed.details };
    }

    const segments = parsed.segments;
    if (op.op === 'set-row') {
      const updated = updateAtPath(result, segments, () => ({ ok: true, node: op.row }));
      if (!updated.ok) {
        this.logError({
          message: 'QueryDelta op failed: set-row',
          queryId,
          op: op.op,
          path: op.path,
          details: updated.details,
        });
      }
      return updated;
    }

    if (op.op === 'remove-row') {
      if (segments.length === 0) {
        return this.failOp(queryId, op, 'Empty path');
      }

      const last = segments[segments.length - 1];
      if (last.type === 'field') {
        const updated = updateAtPath(result, segments, () => ({ ok: true, node: null }));
        if (!updated.ok) {
          this.logError({
            message: 'QueryDelta op failed: remove-row',
            queryId,
            op: op.op,
            path: op.path,
            details: updated.details,
          });
        }
        return updated;
      }

      const listSegments = segments.slice(0, -1);
      const updated = updateAtPath(result, listSegments, (node) => {
        if (!Array.isArray(node)) {
          return { ok: false, details: 'Expected list for row removal' };
        }

        const index = last.type === 'index'
          ? last.index
          : findIndexById(node, last.id);
        if (index < 0 || index >= node.length) {
          return { ok: false, details: 'Row not found for removal' };
        }
        const next = node.slice();
        next.splice(index, 1);
        return { ok: true, node: next };
      });

      if (!updated.ok) {
        this.logError({
          message: 'QueryDelta op failed: remove-row',
          queryId,
          op: op.op,
          path: op.path,
          details: updated.details,
        });
      }
      return updated;
    }

    if (op.op === 'insert-row') {
      const updated = updateAtPath(result, segments, (node) => {
        if (!Array.isArray(node)) {
          return { ok: false, details: 'Expected list for insert-row' };
        }
        const index = Math.min(Math.max(0, op.index), node.length);
        const next = node.slice();
        next.splice(index, 0, op.row);
        return { ok: true, node: next };
      });

      if (!updated.ok) {
        this.logError({
          message: 'QueryDelta op failed: insert-row',
          queryId,
          op: op.op,
          path: op.path,
          details: updated.details,
        });
      }
      return updated;
    }

    if (op.op === 'move-row') {
      const updated = updateAtPath(result, segments, (node) => {
        if (!Array.isArray(node)) {
          return { ok: false, details: 'Expected list for move-row' };
        }
        if (op.from < 0 || op.from >= node.length || op.to < 0 || op.to >= node.length) {
          return { ok: false, details: 'move-row index out of bounds' };
        }
        const next = node.slice();
        const [item] = next.splice(op.from, 1);
        next.splice(op.to, 0, item);
        return { ok: true, node: next };
      });

      if (!updated.ok) {
        this.logError({
          message: 'QueryDelta op failed: move-row',
          queryId,
          op: op.op,
          path: op.path,
          details: updated.details,
        });
      }
      return updated;
    }

    if (op.op === 'remove-row-by-index') {
      const updated = updateAtPath(result, segments, (node) => {
        if (!Array.isArray(node)) {
          return { ok: false, details: 'Expected list for remove-row-by-index' };
        }
        if (op.index < 0 || op.index >= node.length) {
          return { ok: false, details: 'remove-row-by-index out of bounds' };
        }
        const next = node.slice();
        next.splice(op.index, 1);
        return { ok: true, node: next };
      });

      if (!updated.ok) {
        this.logError({
          message: 'QueryDelta op failed: remove-row-by-index',
          queryId,
          op: op.op,
          path: op.path,
          details: updated.details,
        });
      }
      return updated;
    }

    return this.failOp(queryId, op, 'Unknown op');
  }

  private failOp(queryId: string, op: QueryDeltaOp, details: string): UpdateResult {
    this.logError({
      message: 'QueryDelta op failed',
      queryId,
      op: op.op,
      path: 'path' in op ? op.path : undefined,
      details,
    });
    return { ok: false, details };
  }

  private logError(payload: ErrorPayload): void {
    this.logger(payload);
  }
}

const isPlainObject = (value: unknown): value is Record<string, unknown> => {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
};

const updateAtPath = (node: unknown, segments: PathSegment[], updater: UpdateFn): UpdateResult => {
  if (segments.length === 0) {
    return updater(node);
  }

  const [segment, ...rest] = segments;
  if (segment.type === 'field') {
    if (!isPlainObject(node)) {
      return { ok: false, details: `Expected object before field '${segment.name}'` };
    }
    if (!(segment.name in node)) {
      return { ok: false, details: `Missing field '${segment.name}'` };
    }
    const child = node[segment.name];
    const updated = updateAtPath(child, rest, updater);
    if (!updated.ok) {
      return updated;
    }
    if (updated.node === child) {
      return { ok: true, node };
    }
    return { ok: true, node: { ...node, [segment.name]: updated.node } };
  }

  if (!Array.isArray(node)) {
    return { ok: false, details: 'Expected list before index selector' };
  }

  const index =
    segment.type === 'index'
      ? segment.index
      : findIndexById(node, segment.id);
  if (index < 0 || index >= node.length) {
    return { ok: false, details: 'Row not found for selector' };
  }
  const child = node[index];
  const updated = updateAtPath(child, rest, updater);
  if (!updated.ok) {
    return updated;
  }
  if (updated.node === child) {
    return { ok: true, node };
  }
  const next = node.slice();
  next[index] = updated.node;
  return { ok: true, node: next };
};

const parsePath = (path: string): { ok: true; segments: PathSegment[] } | { ok: false; details: string } => {
  if (!path.startsWith('.')) {
    return { ok: false, details: 'Path must start with a dot' };
  }

  const rawSegments = path.slice(1).split('.');
  if (rawSegments.length === 0) {
    return { ok: false, details: 'Empty path' };
  }

  const segments: PathSegment[] = [];
  for (const raw of rawSegments) {
    if (!raw) {
      return { ok: false, details: 'Empty path segment' };
    }

    let cursor = 0;
    let fieldName = '';
    while (cursor < raw.length && raw[cursor] !== '#' && raw[cursor] !== '[') {
      fieldName += raw[cursor];
      cursor += 1;
    }

    if (!fieldName) {
      return { ok: false, details: 'Missing field name' };
    }

    segments.push({ type: 'field', name: fieldName });

    while (cursor < raw.length) {
      if (raw[cursor] === '#') {
        if (raw[cursor + 1] !== '(') {
          return { ok: false, details: 'Invalid id selector' };
        }
        cursor += 2;
        const parsedId = parseEscapedId(raw, cursor);
        if (!parsedId.ok) {
          return { ok: false, details: parsedId.details };
        }
        segments.push({ type: 'id', id: parsedId.id });
        cursor = parsedId.next;
        continue;
      }

      if (raw[cursor] === '[') {
        const closing = raw.indexOf(']', cursor + 1);
        if (closing === -1) {
          return { ok: false, details: 'Unclosed index selector' };
        }
        const rawIndex = raw.slice(cursor + 1, closing);
        if (!rawIndex || !/^[0-9]+$/.test(rawIndex)) {
          return { ok: false, details: 'Invalid index selector' };
        }
        segments.push({ type: 'index', index: Number(rawIndex) });
        cursor = closing + 1;
        continue;
      }

      return { ok: false, details: 'Invalid selector segment' };
    }
  }

  return { ok: true, segments };
};

const parseEscapedId = (
  raw: string,
  start: number
): { ok: true; id: string; next: number } | { ok: false; details: string } => {
  let cursor = start;
  let id = '';
  while (cursor < raw.length) {
    const char = raw[cursor];
    if (char === ')') {
      return { ok: true, id, next: cursor + 1 };
    }
    if (char === '\\') {
      const nextChar = raw[cursor + 1];
      if (nextChar === undefined) {
        return { ok: false, details: 'Dangling escape in id selector' };
      }
      id += nextChar;
      cursor += 2;
      continue;
    }
    id += char;
    cursor += 1;
  }
  return { ok: false, details: 'Unclosed id selector' };
};

const findIndexById = (list: unknown[], id: string): number => {
  return list.findIndex((row) => {
    if (!isPlainObject(row)) {
      return false;
    }
    if (!('id' in row)) {
      return false;
    }
    return String((row as { id: unknown }).id) === id;
  });
};
