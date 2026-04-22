function normalizeBigInt(value: bigint): number {
  return Number(value);
}

function normalizeBufferView(value: ArrayBufferView): number[] {
  return Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
}

export function normalizeForWasmJson(value: unknown): unknown {
  if (value === undefined) {
    return null;
  }

  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return value;
  }

  if (typeof value === "bigint") {
    return normalizeBigInt(value);
  }

  if (value instanceof Date) {
    return value.toISOString();
  }

  if (ArrayBuffer.isView(value)) {
    return normalizeBufferView(value);
  }

  if (value instanceof ArrayBuffer) {
    return Array.from(new Uint8Array(value));
  }

  if (Array.isArray(value)) {
    return value.map((entry) => normalizeForWasmJson(entry));
  }

  if (typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, normalizeForWasmJson(entry)]),
    );
  }

  return String(value);
}
