export type DatabaseId = string;
export type CacheNamespace = string;

export function requireDatabaseId(databaseId: unknown, label = 'databaseId'): DatabaseId {
  if (typeof databaseId !== 'string' || databaseId.trim() === '') {
    throw new Error(`${label} is required`);
  }

  return databaseId;
}

export function resolveEndpointUrl(
  baseUrl: string,
  endpointPath: string,
  params: Record<string, string | undefined> = {}
): string {
  const normalizedBase = baseUrl.endsWith('/') ? baseUrl : `${baseUrl}/`;
  const normalizedPath = endpointPath.replace(/^\/+/, '');
  const url = new URL(normalizedPath, normalizedBase);

  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined) {
      url.searchParams.set(key, value);
    }
  });

  return url.toString();
}

export function withDatabaseId(url: string, databaseId: DatabaseId): string {
  const parsed = new URL(url, 'http://pyre.local');
  parsed.searchParams.set('databaseId', requireDatabaseId(databaseId));

  if (isAbsoluteUrl(url)) {
    return parsed.toString();
  }

  return `${parsed.pathname}${parsed.search}${parsed.hash}`;
}

export function deriveIndexedDbName(
  baseIndexedDbName: string,
  cacheNamespace: CacheNamespace,
  databaseId: DatabaseId
): string {
  const namespace = requireDatabaseId(cacheNamespace, 'cacheNamespace');
  const source = requireDatabaseId(databaseId);
  return [
    safeNamePart(baseIndexedDbName || 'pyre-client'),
    safeNamePart(namespace),
    `${safeNamePart(source)}_${stableHash(source)}`,
  ].join(':');
}

function isAbsoluteUrl(url: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(url);
}

function safeNamePart(value: string): string {
  const safe = value
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .replace(/_+/g, '_');

  return safe || 'default';
}

function stableHash(value: string): string {
  let hash = 5381;

  for (let index = 0; index < value.length; index += 1) {
    hash = ((hash << 5) + hash) ^ value.charCodeAt(index);
  }

  return (hash >>> 0).toString(36);
}
