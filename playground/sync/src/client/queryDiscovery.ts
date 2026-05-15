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

    return discoverQueriesFromMetadataDir(metadataDir);
}

export function discoverQueriesFromMetadataDir(metadataDir: string): QueryMetadata[] {
    const queries: QueryMetadata[] = [];
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

        const inputFields = parseInputFields(content);

        queries.push({
            id: idMatch[1],
            name: `${operation} ${name}`,
            isMutation,
            inputFields: inputFields.length > 0 ? inputFields : undefined,
        });
    }

    return queries.sort((a, b) => a.name.localeCompare(b.name));
}

function parseInputFields(content: string): Array<{ name: string; type: 'string' | 'number' | 'boolean' }> {
    const inputValidatorMatch = content.match(/const RawInputValidator = z\.object\(\{([\s\S]*?)\n\}\);/);
    if (!inputValidatorMatch) {
        return [];
    }

    const inputFields: Array<{ name: string; type: 'string' | 'number' | 'boolean' }> = [];
    const fieldPattern = /(\w+)\s*:\s*([^,\n]+)/g;
    let fieldMatch;
    while ((fieldMatch = fieldPattern.exec(inputValidatorMatch[1])) !== null) {
        const [, fieldName, validator] = fieldMatch;
        inputFields.push({ name: fieldName, type: inputTypeFromZodValidator(validator) });
    }
    return inputFields;
}

function inputTypeFromZodValidator(validator: string): 'string' | 'number' | 'boolean' {
    if (validator.includes('z.number()')) {
        return 'number';
    }
    if (validator.includes('CoercedBool') || validator.includes('z.boolean()')) {
        return 'boolean';
    }
    return 'string';
}
