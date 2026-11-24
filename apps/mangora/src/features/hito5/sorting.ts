import type {
  ExploreSortBy,
  LibrarySortBy,
  SortableExploreItem,
  SortableLibraryItem,
} from "./types";

function compareTitles(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: "base", numeric: true });
}

function parseTimestampMs(value?: string | null): number {
  if (!value) return 0;
  const normalized = value.includes("T") ? value : value.replace(" ", "T");
  const parsed = Date.parse(normalized);
  return Number.isNaN(parsed) ? 0 : parsed;
}

/**
 * Pure comparator-backed sort for Library collections preserving immutability.
 */
export function sortLibraryItems<T extends SortableLibraryItem>(
  items: readonly T[],
  sortBy: LibrarySortBy,
): T[] {
  const copy = [...items];
  copy.sort((a, b) => {
    switch (sortBy) {
      case "title_asc":
        return compareTitles(a.title, b.title);
      case "title_desc":
        return compareTitles(b.title, a.title);
      case "chapters_read_desc": {
        const diff = (b.chapters_read ?? 0) - (a.chapters_read ?? 0);
        return diff !== 0 ? diff : compareTitles(a.title, b.title);
      }
      case "date_added_desc": {
        const diff = parseTimestampMs(b.created_at) - parseTimestampMs(a.created_at);
        return diff !== 0 ? diff : compareTitles(a.title, b.title);
      }
      case "favorites_first": {
        const favDiff = Number(Boolean(b.is_favorite)) - Number(Boolean(a.is_favorite));
        if (favDiff !== 0) return favDiff;
        const tsDiff = parseTimestampMs(b.last_read) - parseTimestampMs(a.last_read);
        return tsDiff !== 0 ? tsDiff : compareTitles(a.title, b.title);
      }
      case "last_read":
      default: {
        const diff = parseTimestampMs(b.last_read) - parseTimestampMs(a.last_read);
        return diff !== 0 ? diff : compareTitles(a.title, b.title);
      }
    }
  });
  return copy;
}

/**
 * Pure sort for Explore / Search results preserving original relevance order when requested.
 */
export function sortExploreItems<T extends SortableExploreItem>(
  items: readonly T[],
  sortBy: ExploreSortBy,
): T[] {
  if (sortBy === "relevance") {
    return [...items];
  }
  const copy = [...items];
  copy.sort((a, b) => {
    switch (sortBy) {
      case "title_asc":
        return compareTitles(a.title, b.title);
      case "title_desc":
        return compareTitles(b.title, a.title);
      case "status_ongoing_first": {
        const aOngoing = (a.status ?? "").toLowerCase().includes("ongoing") ||
          (a.status ?? "").toLowerCase().includes("emisi")
          ? 1
          : 0;
        const bOngoing = (b.status ?? "").toLowerCase().includes("ongoing") ||
          (b.status ?? "").toLowerCase().includes("emisi")
          ? 1
          : 0;
        const diff = bOngoing - aOngoing;
        return diff !== 0 ? diff : compareTitles(a.title, b.title);
      }
      default:
        return 0;
    }
  });
  return copy;
}
