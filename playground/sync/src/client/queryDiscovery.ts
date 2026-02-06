import { readFileSync, readdirSync, existsSync } from "fs";
import { join } from "path";

export interface QueryMetadata {
    id: string; // Hash ID
    name: string; // Human-readable name
    isMutation: boolean;
    inputFields?: Array<{
        name: string;
        type: 'string' | 'number' | 'boolean';
    }>; // Parameter names with types
}

/**
 * Discover queries by parsing generated TypeScript files and query source files
 */
export function discoverQueries(): QueryMetadata[] {
    const queries: QueryMetadata[] = [];

    const metadataDir = join(
        process.cwd(),
        "pyre",
        "generated",
        "typescript",
        "core",
        "queries",
        "metadata"
    );

    if (!existsSync(metadataDir)) {
        console.warn("Generated query metadata not found. Run 'pyre generate' first.");
        return [];
    }

    const metadataFiles = readdirSync(metadataDir).filter((f) => f.endsWith(".ts"));

    for (const file of metadataFiles) {
        const filePath = join(metadataDir, file);
        const content = readFileSync(filePath, "utf-8");

        const operationMatch = content.match(/operation:\s*"(query|insert|update|delete)"/);
        const idMatch = content.match(/id:\s*"([a-f0-9]{64})"/);

        if (!operationMatch || !idMatch) {
            continue;
        }

        const operation = operationMatch[1];
        const name = file.replace(/\.ts$/, "");
        const isMutation = operation !== "query";

        // Extract input fields from InputValidator
        const inputValidatorMatch = content.match(/const InputValidator = Ark\.type\(\{([\s\S]*?)\}\);/);
        const inputFields: Array<{ name: string; type: 'string' | 'number' | 'boolean' }> = [];

        if (inputValidatorMatch) {
            const inputFieldsBlock = inputValidatorMatch[1];
            const fieldPattern = /(\w+)\s*:\s*"([^"]+)"/g;
            let fieldMatch;
            while ((fieldMatch = fieldPattern.exec(inputFieldsBlock)) !== null) {
                const [, fieldName, fieldType] = fieldMatch;
                let type: 'string' | 'number' | 'boolean' = 'string';
                if (fieldType === "number") {
                    type = 'number';
                } else if (fieldType === "boolean") {
                    type = 'boolean';
                }
                inputFields.push({ name: fieldName, type });
            }
        }

        queries.push({
            id: idMatch[1],
            name: `${operation} ${name}`,
            isMutation,
            inputFields: inputFields.length > 0 ? inputFields : undefined,
        });
    }

    return queries.sort((a, b) => a.name.localeCompare(b.name));
}
