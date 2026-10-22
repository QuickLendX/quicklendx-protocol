import { z } from 'zod';
import { Page } from './schemas';

export type Page<T> = {
  data: T[];
  nextCursor: string | null;
  hasMore: boolean;
  freshness?: any;
  system?: any;
};

/**
 * Async generator that iterates over pages returned by a paginated endpoint.
 * Stops when `nextCursor` is null or when `maxPages` is reached.
 */
export async function* paginate<T>(
  fetchPage: (cursor?: string) => Promise<Page<T>>,
  maxPages = 50,
): AsyncGenerator<T[], void, unknown> {
  let cursor: string | undefined = undefined;
  let pagesFetched = 0;
  while (pagesFetched < maxPages) {
    const page = await fetchPage(cursor);
    yield page.data;
    pagesFetched++;
    if (!page.nextCursor) break;
    cursor = page.nextCursor;
  }
}
