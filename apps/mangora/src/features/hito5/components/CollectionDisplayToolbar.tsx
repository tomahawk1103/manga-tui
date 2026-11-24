import { getHito5Labels } from "../palettes";
import type {
  CardDensity,
  ExploreSortBy,
  LibrarySortBy,
  LibraryViewMode,
  UiLanguage,
} from "../types";

export interface CollectionDisplayToolbarProps {
  mode: "library" | "explore";
  language?: UiLanguage;
  density: CardDensity;
  onDensityChange: (density: CardDensity) => void;
  viewMode?: LibraryViewMode;
  onViewModeChange?: (viewMode: LibraryViewMode) => void;
  sortBy: LibrarySortBy | ExploreSortBy;
  onSortChange: (sortBy: LibrarySortBy | ExploreSortBy) => void;
  itemCount?: number;
}

/**
 * Compact editorial toolbar for switching density, Grid/List view mode, and custom sort order
 * in Mangora's Library and Explore screens.
 */
export function CollectionDisplayToolbar({
  mode,
  language = "es",
  density,
  onDensityChange,
  viewMode = "grid",
  onViewModeChange,
  sortBy,
  onSortChange,
  itemCount,
}: CollectionDisplayToolbarProps) {
  const labels = getHito5Labels(language);

  const densityOptions: Array<{ value: CardDensity; label: string }> = [
    { value: "compact", label: labels.densityCompact },
    { value: "comfortable", label: labels.densityComfortable },
    { value: "spacious", label: labels.densitySpacious },
  ];

  const sortEntries =
    mode === "library"
      ? (Object.entries(labels.librarySortOptions) as Array<[LibrarySortBy, string]>)
      : (Object.entries(labels.exploreSortOptions) as Array<[ExploreSortBy, string]>);

  return (
    <div
      className="mangora-toolbar mangora-hito5-root"
      role="region"
      aria-label={mode === "library" ? labels.librarySection : labels.exploreSection}
    >
      <div className="mangora-toolbar-section">
        <span className="mangora-mono-meta">{labels.densityLabel}:</span>
        <div className="mangora-chip-group" role="group" aria-label={labels.densityLabel}>
          {densityOptions.map((opt) => (
            <button
              key={opt.value}
              type="button"
              className="mangora-chip"
              aria-pressed={density === opt.value}
              onClick={() => onDensityChange(opt.value)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {mode === "library" && onViewModeChange && (
        <div className="mangora-toolbar-section">
          <span className="mangora-mono-meta">{labels.viewModeLabel}:</span>
          <div className="mangora-chip-group" role="group" aria-label={labels.viewModeLabel}>
            <button
              type="button"
              className="mangora-chip"
              aria-pressed={viewMode === "grid"}
              onClick={() => onViewModeChange("grid")}
            >
              {labels.viewGrid}
            </button>
            <button
              type="button"
              className="mangora-chip"
              aria-pressed={viewMode === "list"}
              onClick={() => onViewModeChange("list")}
            >
              {labels.viewList}
            </button>
          </div>
        </div>
      )}

      <div className="mangora-toolbar-section">
        {typeof itemCount === "number" && (
          <span className="mangora-mono-meta">
            {itemCount} {language === "en" ? "items" : "series"}
          </span>
        )}
        <label className="mangora-mono-meta" htmlFor={`mangora-sort-${mode}`}>
          {labels.sortLabel}:
        </label>
        <select
          id={`mangora-sort-${mode}`}
          className="mangora-select"
          value={sortBy}
          onChange={(e) => onSortChange(e.target.value as LibrarySortBy | ExploreSortBy)}
        >
          {sortEntries.map(([val, label]) => (
            <option key={val} value={val}>
              {label}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
