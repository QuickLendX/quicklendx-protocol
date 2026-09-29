declare module 'better-sqlite3' {
  interface Database {
    exec(sql: string): this;
    prepare(sql: string): Statement;
    pragma(pragma: string): any;
    close(): void;
    transaction<T>(fn: () => T): () => T;
  }

  interface Statement {
    run(...params: any[]): { lastInsertRowId: number; changes: number };
    get<T = any>(...params: any[]): T | undefined;
    all<T = any>(...params: any[]): T[];
  }

  export default function Database(path: string): Database;
}
