import { mkdtempSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { expect, test } from "bun:test";
import { discoverQueriesFromMetadataDir } from "./queryDiscovery";

test("discovers required fields from generated mutation metadata", () => {
    const dir = mkdtempSync(join(tmpdir(), "pyre-query-discovery-"));
    try {
        writeFileSync(
            join(dir, "postCreate.ts"),
            `import { z } from "zod";

const CoercedBool = z.boolean();
const RawInputValidator = z.object({
  title: z.string(),
  content: z.string(),
  published: CoercedBool
});
const InputValidator = RawInputValidator;

export const meta = {
  id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  operation: "insert" as const,
  InputValidator,
};
`
        );

        expect(discoverQueriesFromMetadataDir(dir)).toEqual([
            {
                id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                name: "insert postCreate",
                isMutation: true,
                inputFields: [
                    { name: "title", type: "string" },
                    { name: "content", type: "string" },
                    { name: "published", type: "boolean" },
                ],
            },
        ]);
    } finally {
        rmSync(dir, { recursive: true, force: true });
    }
});
