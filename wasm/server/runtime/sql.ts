export type SqlInfo = {
  include: boolean;
  params: string[];
  sql: string;
};

export type SqlStatement = { sql: string; args: Record<string, any> };

function stringifyNestedObjects(obj: Record<string, any>): Record<string, any> {
  const result: Record<string, any> = {};

  for (const key in obj) {
    if (Object.prototype.hasOwnProperty.call(obj, key)) {
      const value = obj[key];
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        result[key] = JSON.stringify(value);
      } else {
        result[key] = value;
      }
    }
  }

  return result;
}

export function toSessionArgs(sessionArgs: string[], session: Record<string, any>): Record<string, any> {
  const result: Record<string, any> = {};

  if (session == null) {
    return result;
  }

  for (const key of sessionArgs) {
    if (key in session) {
      result[`session_${key}`] = session[key];
    }
  }

  return result;
}

export function buildArgs(
  input: Record<string, any> | undefined,
  session: Record<string, any>,
  sessionArgs: string[]
): Record<string, any> {
  const args: Record<string, any> = {};

  if (input) {
    for (const [key, value] of Object.entries(input)) {
      if (value !== undefined) {
        args[key] = value;
      }
    }
  }

  Object.assign(args, toSessionArgs(sessionArgs, session));

  return stringifyNestedObjects(args);
}

export function toSqlStatements(sql: SqlInfo[], args: Record<string, any>): SqlStatement[] {
  return sql.map(({ sql: statement, params }) => {
    const filtered: Record<string, any> = {};
    for (const key of params) {
      if (key in args) {
        filtered[key] = args[key];
      }
    }
    return { sql: statement, args: filtered };
  });
}

export function formatResultData(sql: SqlInfo[], resultSets: any[]): Record<string, any> {
  const formatted: Record<string, any> = {};
  const values = resultSets.filter((_, index) => sql[index]?.include);

  for (const resultSet of values) {
    if (!resultSet?.columns?.length) {
      continue;
    }
    const colName = resultSet.columns[0];
    if (colName.startsWith('_')) {
      continue;
    }
    for (const row of resultSet.rows || []) {
      if (colName in row && typeof row[colName] === 'string') {
        const parsed = JSON.parse(row[colName]);
        formatted[colName] = Array.isArray(parsed) ? parsed : [parsed];
        break;
      }
    }
  }
  return formatted;
}
