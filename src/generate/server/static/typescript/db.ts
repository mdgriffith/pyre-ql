import * as LibSql from '@libsql/client';
import * as Env from './db/env'
import * as Ark from 'arktype';
import * as Watched from './watched';

export type ExecuteResult = SuccessResult | ErrorResult;

export interface SuccessResult {
  kind: 'success';
  metadata: {
    outOfDate: boolean;
    watched: Watched.Watched[];
  };
  data: LibSql.ResultSet[];
}

export interface ErrorResult {
  kind: 'error';
  errorType: ErrorType;
  message: string;
}

export enum ErrorType {
  NotFound,
  Unauthorized,
  InvalidInput,
  InvalidSession,
  UnknownError,
  UnknownQuery,
  NoDatabase
}

export interface Runner<session, input, output> {
  id: string;
  primary_db: Env.DatabaseKey;
  attached_dbs: Env.DatabaseKey[];
  session: Ark.Type<session>;
  session_args: string[];
  input: Ark.Type<input>;
  output: Ark.Type<output>;
  run: (env: Env.Config, session: session, args: any) => Promise<ExecuteResult>;
}

export type SqlInfo = {
  include: boolean;
  params: Array<string>;
  sql: string;
}

export type ToRunnerArgs<session, input, output> = {
  id: string;
  primary_db: Env.DatabaseKey;
  attached_dbs: Env.DatabaseKey[];
  session: Ark.Type<session>;
  session_args: string[];
  input: Ark.Type<input>;
  output: Ark.Type<output>;
  sql: Array<SqlInfo>;
  watch_triggers: Watched.Watched[];
};

export const to_runner = <Session, Input, Output>(options: ToRunnerArgs<Session, Input, Output>): Runner<Session, Input, Output> => {
  return {
    id: options.id,
    primary_db: options.primary_db,
    attached_dbs: options.attached_dbs,
    session: options.session,
    session_args: options.session_args,
    input: options.input,
    output: options.output,
    run: async (env: Env.Config, session: Session, input: any): Promise<ExecuteResult> => {
      // Validate session
      const validated_input: any | Ark.ArkErrors = options.input(input);

      if (validated_input instanceof Ark.type.errors) {
        // Extract detailed error message from arktype
        let errorMessage = 'Invalid input';
        try {
          const errors = validated_input;
          if (errors && typeof errors === 'object') {
            const errorStr = JSON.stringify(errors, null, 2);
            errorMessage = `Validation failed: ${errorStr}`;
          } else if (typeof errors === 'string') {
            errorMessage = `Validation failed: ${errors}`;
          }
        } catch (e) {
          // Fallback to a generic message if we can't stringify
          errorMessage = 'Input validation failed. Check that all required fields are present and have correct types.';
        }
        return {
          kind: 'error',
          errorType: ErrorType.InvalidInput,
          message: errorMessage,
        };
      }

      // Validate session
      const validated_session: any | Ark.ArkErrors = options.session(session);
      if (validated_session instanceof Ark.type.errors) {
        return {
          kind: 'error',
          errorType: ErrorType.InvalidSession,
          message: 'Expected object',
        };
      }

      // Validate that we have
      for (const db of options.attached_dbs) {
        if (db in env) {
          continue
        }
        return {
          kind: 'error',
          errorType: ErrorType.NoDatabase,
          message: `No instance of ${db} provided`,
        };
      }

      const valid_session_args = to_session_args(options.session_args, validated_session);
      const attached_database_args = to_database_args(options.attached_dbs, env);

      const valid_args = stringify_nested_objects({ ...validated_input, ...valid_session_args, ...attached_database_args });

      // Finished validation, let's prepare the statements.

      const sql_arg_list: LibSql.InStatement[] = options.sql.map(({ params, sql }) => {
        const filtered_args: Record<string, any> = {};
        for (const key of params) {
          if (key in valid_args) {
            filtered_args[key] = valid_args[key];
          }
        }

        return { sql: sql, args: filtered_args };
      });

      const lib_sql_config = Env.to_libSql_config(env, options.primary_db);
      if (lib_sql_config == undefined) {
        return {
          kind: "error",
          errorType: ErrorType.NoDatabase,
          message: `${options.primary_db} database was not provided!`
        }
      }

      // Done validating, let's talk to the db.
      const client = LibSql.createClient(lib_sql_config);
      try {
        const res = await client.batch(sql_arg_list);

        const formatted_return_data: any = {}

        for (const result_set of only_included(options.sql, res)) {
          // the generated sql formats the data as JSON
          // But it comes over as string encoded JSON
          // So we need to parse it

          if (result_set.columns.length < 1) { continue }
          const col_name = result_set.columns[0];
          const gathered_rows: any[] = [];

          for (const row of result_set.rows) {
            console.log(row);
            if (col_name in row && typeof row[col_name] == 'string') {
              gathered_rows.push(JSON.parse(row[col_name]));
            }
          }
          formatted_return_data[col_name] = gathered_rows;
        }


        return {
          kind: 'success',
          metadata: { outOfDate: false, watched: options.watch_triggers },
          data: formatted_return_data
        };
      } catch (error) {
        console.log(error);
        return {
          kind: 'error',
          errorType: ErrorType.InvalidInput,
          message: 'Database error',
        };
      }

    },
  };
};

type KeyValues = { [key: string]: string };

const to_session_args = (allowed_keys: string[], session: any): KeyValues => {
  if (session == null) {
    return {};
  }

  const session_args: KeyValues = {};
  for (const key of allowed_keys) {
    session_args['session_' + key] = session[key];
  }
  return session_args;
};

const to_database_args = (attached_databases: Env.DatabaseKey[], env: Env.Config): KeyValues => {
  const db_args: KeyValues = {};
  for (const db_key of attached_databases) {
    if (db_key in env && env[db_key] != undefined) {
      db_args['db_' + db_key] = env[db_key].id;
    }
  }
  return db_args;
};

const stringify_nested_objects = (obj: Record<string, any>): Record<string, any> => {
  const result: Record<string, any> = {};

  for (const key in obj) {
    if (obj.hasOwnProperty(key)) {
      const value = obj[key];
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        result[key] = JSON.stringify(value);
      } else {
        result[key] = value;
      }
    }
  }

  return result;
};


function only_included(sqlItems: SqlInfo[], values: any[]): any[] {
  return values.filter((_, index) => sqlItems[index]?.include);
}