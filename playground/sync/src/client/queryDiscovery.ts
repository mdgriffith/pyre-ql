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

    // Path to generated query.ts
    const queryTsPath = join(process.cwd(), "pyre", "generated", "server", "typescript", "query.ts");

    if (!existsSync(queryTsPath)) {
        console.warn("Generated query.ts not found. Run 'pyre generate' first.");
        return [];
    }

    const queryTsContent = readFileSync(queryTsPath, "utf-8");

    // Parse switch statement to extract query ID -> name mapping
    // Pattern: case "hash": return QueryName.query.run(...)
    const casePattern = /case\s+"([^"]+)":\s*return\s+(\w+)\.query\.run/g;
    const idToName = new Map<string, string>();

    let match;
    while ((match = casePattern.exec(queryTsContent)) !== null) {
        const [, hashId, queryName] = match;
        idToName.set(hashId, queryName);
    }

    // Read query source files to get metadata
    const queriesDir = join(process.cwd(), "pyre", "queries");
    if (!existsSync(queriesDir)) {
        return [];
    }

    const queryFiles = readdirSync(queriesDir).filter((f) => f.endsWith(".pyre"));

    for (const file of queryFiles) {
        const filePath = join(queriesDir, file);
        const content = readFileSync(filePath, "utf-8");

        // Extract query/mutation name and type
        const queryMatch = content.match(/^(query|insert|update|delete)\s+(\w+)/m);
        if (!queryMatch) continue;

        const [, operation, name] = queryMatch;
        const isMutation = operation !== "query";

        // Extract parameter names (e.g., $name: String)
        const paramPattern = /\$(\w+)\s*:\s*(\w+)/g;
        const inputFields: Array<{ name: string; type: 'string' | 'number' | 'boolean' }> = [];
        let paramMatch;
        while ((paramMatch = paramPattern.exec(content)) !== null) {
            const [, paramName, paramType] = paramMatch;
            // Map Pyre types to TypeScript types
            let tsType: 'string' | 'number' | 'boolean' = 'string';
            if (paramType === 'Bool' || paramType === 'Boolean') {
                tsType = 'boolean';
            } else if (paramType === 'Int' || paramType === 'Float' || paramType === 'Number') {
                tsType = 'number';
            } else if (paramType === 'String' || paramType === 'Text') {
                tsType = 'string';
            }
            inputFields.push({ name: paramName, type: tsType });
        }

        // Find the hash ID for this query
        // Try to match by reading the generated query file
        const generatedQueryPath = join(
            process.cwd(),
            "pyre",
            "generated",
            "server",
            "typescript",
            "query",
            `${name.charAt(0).toLowerCase() + name.slice(1)}.ts`
        );

        let hashId: string | undefined;
        let inputFieldTypes: Record<string, 'string' | 'number' | 'boolean'> = {};

        // Try to find hash ID from the generated query file
        if (existsSync(generatedQueryPath)) {
            const generatedContent = readFileSync(generatedQueryPath, "utf-8");
            // Look for the id field in the query runner (Db.to_runner({ id: "..." }))
            // Match id: followed by a long hex hash string (64 hex chars) to distinguish from type definitions
            // Query IDs are always 64-character hex strings, while type definitions use short strings like "number"
            // We match the 64-char hex pattern which should be unique to query IDs
            const hashMatch = generatedContent.match(/id:\s*"([a-f0-9]{64})"/);
            if (hashMatch) {
                hashId = hashMatch[1];
            }

            // Extract Input type definition to get accurate types
            const inputMatch = generatedContent.match(/export const Input = Ark\.type\(\{([^}]+)\}\)/s);
            if (inputMatch) {
                const inputDef = inputMatch[1];
                // Parse field definitions like "title": "string", "published": "boolean"
                const fieldPattern = /"([^"]+)":\s*"([^"]+)"/g;
                let fieldMatch;
                while ((fieldMatch = fieldPattern.exec(inputDef)) !== null) {
                    const [, fieldName, fieldType] = fieldMatch;
                    if (fieldType === 'boolean') {
                        inputFieldTypes[fieldName] = 'boolean';
                    } else if (fieldType === 'number') {
                        inputFieldTypes[fieldName] = 'number';
                    } else {
                        inputFieldTypes[fieldName] = 'string';
                    }
                }
            }

            // Update inputFields with accurate types from generated file
            if (Object.keys(inputFieldTypes).length > 0) {
                inputFields.forEach(field => {
                    if (inputFieldTypes[field.name]) {
                        field.type = inputFieldTypes[field.name];
                    }
                });
            }
        }

        // Also check the query.ts switch statement directly as fallback
        if (!hashId) {
            // Look for the query name in imports and match to case statement
            const importMatch = queryTsContent.match(new RegExp(`import.*as\\s+${name}\\s+from`));
            if (importMatch) {
                // Find the case that uses this import
                const caseRegex = new RegExp(`case\\s+"([^"]+)":\\s*return\\s+${name}\\.query\\.run`, "g");
                const caseMatch = caseRegex.exec(queryTsContent);
                if (caseMatch) {
                    hashId = caseMatch[1];
                }
            }
        }

        // Final fallback: search all cases for this query name
        if (!hashId) {
            for (const [id, qName] of idToName.entries()) {
                // Handle camelCase matching (e.g., "UserNew" matches "userNew")
                const camelName = name.charAt(0).toLowerCase() + name.slice(1);
                if (qName === name || qName === camelName || qName.toLowerCase() === name.toLowerCase()) {
                    hashId = id;
                    break;
                }
            }
        }

        // Fallback: try to find by name in the switch statement
        if (!hashId) {
            for (const [id, qName] of idToName.entries()) {
                if (qName === name) {
                    hashId = id;
                    break;
                }
            }
        }

        if (hashId) {
            queries.push({
                id: hashId,
                name: `${operation} ${name}`,
                isMutation,
                inputFields: inputFields.length > 0 ? inputFields : undefined,
            });
        }
    }

    return queries;
}

