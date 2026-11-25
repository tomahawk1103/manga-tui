interface OpenLibraryResult {
  key: string;
  title: string;
  author_name?: string[];
  cover_i?: number;
}

export async function searchSeries(title: string, signal: AbortSignal) {
  const url = new URL("https://openlibrary.org/search.json");
  url.search = new URLSearchParams({
    title,
    fields: "key,title,author_name,cover_i",
    limit: "12",
  }).toString();
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Open Library: ${response.status}`);
  const { docs }: { docs: OpenLibraryResult[] } = await response.json();
  return docs.filter((item) => /^\/works\/OL\d+W$/.test(item.key)).map((item) => ({
    url: new URL(item.key, url.origin).href,
    title: item.title,
    author: item.author_name?.join(", "),
    cover: item.cover_i ? `https://covers.openlibrary.org/b/id/${item.cover_i}-S.jpg` : undefined,
  }));
}
