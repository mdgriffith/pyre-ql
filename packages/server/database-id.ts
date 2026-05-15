export type DatabaseId = string;

export function requireDatabaseId(value: unknown, label = "databaseId"): DatabaseId {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`${label} is required`);
  }

  return value;
}

export function databaseIdFromUrl(url: string | URL): DatabaseId {
  const parsed = typeof url === "string" ? new URL(url, "http://pyre.local") : url;
  return requireDatabaseId(parsed.searchParams.get("databaseId"));
}

export function withDatabaseId<T extends Record<string, unknown>>(databaseId: DatabaseId, message: T): T & { databaseId: DatabaseId } {
  return {
    ...message,
    databaseId: requireDatabaseId(databaseId),
  };
}
