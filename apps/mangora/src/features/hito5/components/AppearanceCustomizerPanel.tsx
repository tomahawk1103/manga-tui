import React from "react";
import { getHito5Labels } from "../palettes";
import type {
  CardDensity,
  ExploreSortBy,
  LibrarySortBy,
  LibraryViewMode,
  UiLanguage,
} from "../types";
import { useMangoraAppearance } from "../useMangoraAppearance";

export interface AppearanceCustomizerPanelProps {
  language?: UiLanguage;
}

/**
 * Editorial settings panel for Hito 5 visual customization:
 * - 5 paper/ink palettes with live swatch previews
 * - Card density, view mode, and custom sorting for Library and Explore
 * - Subtle reader backgrounds and screentone accent toggle
 */
export function AppearanceCustomizerPanel({ language = "es" }: AppearanceCustomizerPanelProps) {
  const {
    config,
    palettes,
    readerBackgrounds,
    isSaving,
    saveError,
    updateAppearance,
    resetAppearance,
  } = useMangoraAppearance();

  const labels = getHito5Labels(language);

  const densities: Array<{ value: CardDensity; label: string }> = [
    { value: "compact", label: labels.densityCompact },
    { value: "comfortable", label: labels.densityComfortable },
    { value: "spacious", label: labels.densitySpacious },
  ];

  const viewModes: Array<{ value: LibraryViewMode; label: string }> = [
    { value: "grid", label: labels.viewGrid },
    { value: "list", label: labels.viewList },
  ];

  return (
    <section className="mangora-hito5-root mangora-card" aria-labelledby="mangora-appearance-title">
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: "1rem",
          marginBottom: "1.25rem",
        }}
      >
        <div>
          <p className="mangora-eyebrow">{labels.appearanceEyebrow}</p>
          <h2 id="mangora-appearance-title" className="mangora-editorial-title" style={{ fontSize: "2rem" }}>
            {labels.appearanceHeadingPrefix} <em>{labels.appearanceHeadingAccent}</em>
          </h2>
          <p style={{ color: "var(--muted)", maxWidth: "56ch", margin: 0 }}>{labels.appearanceSubtitle}</p>
        </div>

        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: "0.45rem" }}>
          <button
            type="button"
            className="mangora-btn-secondary"
            onClick={() => void resetAppearance()}
          >
            {labels.resetDefaults}
          </button>
          <span className="mangora-mono-meta">
            {isSaving ? labels.savingText : saveError ? saveError : labels.savedText}
          </span>
        </div>
      </div>

      {/* Palettes */}
      <div style={{ marginBottom: "1.5rem" }}>
        <h3
          style={{
            fontFamily: "var(--mono)",
            fontSize: "0.86rem",
            textTransform: "uppercase",
            letterSpacing: "0.05em",
            marginBottom: "0.75rem",
          }}
        >
          {labels.paletteSection}
        </h3>
        <div className="mangora-palette-grid">
          {palettes.map((palette) => {
            const active = config.palette === palette.id;
            const title = language === "en" ? palette.name_en : palette.name_es;
            const desc = language === "en" ? palette.description_en : palette.description_es;
            return (
              <button
                key={palette.id}
                type="button"
                className="mangora-palette-card"
                aria-pressed={active}
                onClick={() => void updateAppearance({ palette: palette.id })}
              >
                <div
                  aria-hidden="true"
                  style={{
                    display: "flex",
                    height: "2.25rem",
                    border: "1px solid var(--ink)",
                    overflow: "hidden",
                  }}
                >
                  <span style={{ flex: 2, background: palette.paper }} />
                  <span style={{ flex: 1, background: palette.surface }} />
                  <span style={{ flex: 1, background: palette.ink }} />
                  <span style={{ flex: 1, background: palette.red }} />
                </div>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "0.5rem" }}>
                  <strong>{title}</strong>
                  <span className="mangora-mono-meta">{active ? "ACTIVO" : palette.is_dark ? "DARK" : "LIGHT"}</span>
                </div>
                <span style={{ fontSize: "0.84rem", color: "var(--muted)" }}>{desc}</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Library & Explore Controls */}
      <div className="mangora-two-col" style={{ marginBottom: "1.5rem" }}>
        <div className="mangora-surface-card">
          <h3 style={{ marginTop: 0, marginBottom: "0.85rem", fontSize: "1.15rem" }}>
            {labels.librarySection}
          </h3>
          <div style={{ display: "grid", gap: "0.85rem" }}>
            <div>
              <div className="mangora-mono-meta" style={{ marginBottom: "0.35rem" }}>
                {labels.densityLabel}
              </div>
              <div className="mangora-chip-group">
                {densities.map((d) => (
                  <button
                    key={d.value}
                    type="button"
                    className="mangora-chip"
                    aria-pressed={config.library_density === d.value}
                    onClick={() => void updateAppearance({ library_density: d.value })}
                  >
                    {d.label}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <div className="mangora-mono-meta" style={{ marginBottom: "0.35rem" }}>
                {labels.viewModeLabel}
              </div>
              <div className="mangora-chip-group">
                {viewModes.map((vm) => (
                  <button
                    key={vm.value}
                    type="button"
                    className="mangora-chip"
                    aria-pressed={config.library_view_mode === vm.value}
                    onClick={() => void updateAppearance({ library_view_mode: vm.value })}
                  >
                    {vm.label}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <label
                htmlFor="mangora-pref-library-sort"
                className="mangora-mono-meta"
                style={{ display: "block", marginBottom: "0.35rem" }}
              >
                {labels.sortLabel}
              </label>
              <select
                id="mangora-pref-library-sort"
                className="mangora-select"
                style={{ width: "100%" }}
                value={config.library_sort}
                onChange={(e) =>
                  void updateAppearance({ library_sort: e.target.value as LibrarySortBy })
                }
              >
                {(Object.entries(labels.librarySortOptions) as Array<[LibrarySortBy, string]>).map(
                  ([key, label]) => (
                    <option key={key} value={key}>
                      {label}
                    </option>
                  ),
                )}
              </select>
            </div>
          </div>
        </div>

        <div className="mangora-surface-card">
          <h3 style={{ marginTop: 0, marginBottom: "0.85rem", fontSize: "1.15rem" }}>
            {labels.exploreSection}
          </h3>
          <div style={{ display: "grid", gap: "0.85rem" }}>
            <div>
              <div className="mangora-mono-meta" style={{ marginBottom: "0.35rem" }}>
                {labels.densityLabel}
              </div>
              <div className="mangora-chip-group">
                {densities.map((d) => (
                  <button
                    key={d.value}
                    type="button"
                    className="mangora-chip"
                    aria-pressed={config.explore_density === d.value}
                    onClick={() => void updateAppearance({ explore_density: d.value })}
                  >
                    {d.label}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <label
                htmlFor="mangora-pref-explore-sort"
                className="mangora-mono-meta"
                style={{ display: "block", marginBottom: "0.35rem" }}
              >
                {labels.sortLabel}
              </label>
              <select
                id="mangora-pref-explore-sort"
                className="mangora-select"
                style={{ width: "100%" }}
                value={config.explore_sort}
                onChange={(e) =>
                  void updateAppearance({ explore_sort: e.target.value as ExploreSortBy })
                }
              >
                {(Object.entries(labels.exploreSortOptions) as Array<[ExploreSortBy, string]>).map(
                  ([key, label]) => (
                    <option key={key} value={key}>
                      {label}
                    </option>
                  ),
                )}
              </select>
            </div>

            <label
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: "0.65rem",
                padding: "0.65rem",
                border: "1px solid var(--ink)",
                background: "var(--paper)",
                cursor: "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={config.show_screentone_accents}
                onChange={(e) =>
                  void updateAppearance({ show_screentone_accents: e.target.checked })
                }
                style={{ marginTop: "0.2rem", accentColor: "var(--red)" }}
              />
              <span>
                <strong style={{ display: "block", fontSize: "0.92rem" }}>
                  {labels.screentoneToggle}
                </strong>
                <span style={{ fontSize: "0.82rem", color: "var(--muted)" }}>
                  {labels.screentoneHelp}
                </span>
              </span>
            </label>
          </div>
        </div>
      </div>

      {/* Subtle Reader Backgrounds */}
      <div className="mangora-accent-block">
        <h3 style={{ marginTop: 0, marginBottom: "0.75rem", fontSize: "1.15rem" }}>
          {labels.readerSection}
        </h3>
        <div className="mangora-palette-grid">
          {readerBackgrounds.map((bg) => {
            const active = config.reader_background === bg.id;
            const title = language === "en" ? bg.name_en : bg.name_es;
            const desc = language === "en" ? bg.description_en : bg.description_es;
            return (
              <button
                key={bg.id}
                type="button"
                className="mangora-palette-card"
                aria-pressed={active}
                onClick={() => void updateAppearance({ reader_background: bg.id })}
              >
                <div
                  aria-hidden="true"
                  className={`mangora-reader-bg--${bg.id}`}
                  style={{
                    height: "2rem",
                    border: "1px solid var(--ink)",
                  }}
                />
                <strong>{title}</strong>
                <span style={{ fontSize: "0.82rem", color: "var(--muted)" }}>{desc}</span>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}
