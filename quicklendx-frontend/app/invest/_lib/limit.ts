/**
 * Promise pool to run async tasks with at most `concurrency` tasks in-flight at once.
 * Preserves submission order of returned promises.
 * If one task rejects, only that promise rejects while other tasks continue to run.
 */
export function pLimit(concurrency: number) {
  if (concurrency < 1) {
    throw new Error("Concurrency limit must be at least 1");
  }

  const queue: Array<() => void> = [];
  let activeCount = 0;

  const next = () => {
    activeCount--;
    if (queue.length > 0) {
      const task = queue.shift()!;
      task();
    }
  };

  const run = async <T>(
    fn: () => Promise<T>,
    resolve: (value: T) => void,
    reject: (reason?: unknown) => void
  ) => {
    activeCount++;
    try {
      const result = await fn();
      resolve(result);
    } catch (err) {
      reject(err);
    } finally {
      next();
    }
  };

  const enqueue = <T>(fn: () => Promise<T>): Promise<T> => {
    return new Promise<T>((resolve, reject) => {
      const task = () => run(fn, resolve, reject);
      if (activeCount < concurrency) {
        task();
      } else {
        queue.push(task);
      }
    });
  };

  return enqueue;
}
