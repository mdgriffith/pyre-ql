// @ts-nocheck
import { expect, test } from "bun:test";

import { databaseIdFromUrl, requireDatabaseId, withDatabaseId } from "./database-id";

test("requireDatabaseId rejects missing values", () => {
  expect(() => requireDatabaseId(null)).toThrow("databaseId is required");
  expect(() => requireDatabaseId("")).toThrow("databaseId is required");
  expect(() => requireDatabaseId("   ")).toThrow("databaseId is required");
});

test("databaseIdFromUrl reads databaseId query parameter", () => {
  expect(databaseIdFromUrl("https://api.example.test/sync?databaseId=campaign%3A123")).toBe("campaign:123");
  expect(databaseIdFromUrl(new URL("https://api.example.test/db/Create?databaseId=main"))).toBe("main");
});

test("databaseIdFromUrl rejects urls without databaseId", () => {
  expect(() => databaseIdFromUrl("https://api.example.test/sync")).toThrow("databaseId is required");
});

test("withDatabaseId stamps message envelopes", () => {
  expect(withDatabaseId("campaign:123", { type: "delta", data: [] })).toEqual({
    type: "delta",
    databaseId: "campaign:123",
    data: [],
  });
});
