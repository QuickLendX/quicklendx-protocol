import { paginate } from "../pagination";

type FetchPage = Parameters<typeof paginate<number>>[0];

function mockFetchPage() {
  return jest.fn<ReturnType<FetchPage>, Parameters<FetchPage>>();
}

async function collectPages(
  pages: AsyncIterable<number[]>
): Promise<number[][]> {
  const results: number[][] = [];
  for await (const page of pages) {
    results.push(page);
  }
  return results;
}

describe("paginate", () => {
  it("yields a single page and starts without a cursor", async () => {
    const fetchPage = mockFetchPage().mockResolvedValueOnce({
      data: [11, 12],
      nextCursor: null,
      hasMore: false,
    });

    await expect(collectPages(paginate(fetchPage))).resolves.toEqual([
      [11, 12],
    ]);
    expect(fetchPage.mock.calls).toEqual([[undefined]]);
  });

  it("follows each cursor and yields the pages in order", async () => {
    const fetchPage = mockFetchPage()
      .mockResolvedValueOnce({
        data: [11],
        nextCursor: "after-11",
        hasMore: true,
      })
      .mockResolvedValueOnce({
        data: [12],
        nextCursor: "after-12",
        hasMore: true,
      })
      .mockResolvedValueOnce({ data: [13], nextCursor: null, hasMore: false });

    await expect(collectPages(paginate(fetchPage))).resolves.toEqual([
      [11],
      [12],
      [13],
    ]);
    expect(fetchPage.mock.calls).toEqual([
      [undefined],
      ["after-11"],
      ["after-12"],
    ]);
  });

  it("stops on a null cursor even when the page is empty and hasMore is true", async () => {
    const fetchPage = mockFetchPage().mockResolvedValueOnce({
      data: [],
      nextCursor: null,
      hasMore: true,
    });

    await expect(collectPages(paginate(fetchPage))).resolves.toEqual([[]]);
    expect(fetchPage.mock.calls).toEqual([[undefined]]);
  });

  it("stops after maxPages even when every page provides another cursor", async () => {
    let page = 0;
    const fetchPage = mockFetchPage().mockImplementation(async () => {
      page += 1;
      return { data: [page], nextCursor: `after-${page}`, hasMore: true };
    });

    const iterator = paginate(fetchPage, 2);
    await expect(iterator.next()).resolves.toEqual({ done: false, value: [1] });
    await expect(iterator.next()).resolves.toEqual({ done: false, value: [2] });
    await expect(iterator.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
    expect(fetchPage.mock.calls).toEqual([[undefined], ["after-1"]]);
  });

  it("propagates a fetch failure after yielding earlier pages", async () => {
    const error = new Error("fetch failed");
    const fetchPage = mockFetchPage()
      .mockResolvedValueOnce({
        data: [11],
        nextCursor: "after-11",
        hasMore: true,
      })
      .mockRejectedValueOnce(error);
    const iterator = paginate(fetchPage);

    await expect(iterator.next()).resolves.toEqual({
      done: false,
      value: [11],
    });
    await expect(iterator.next()).rejects.toBe(error);
    expect(fetchPage.mock.calls).toEqual([[undefined], ["after-11"]]);
  });
});
