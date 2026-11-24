import "./palettes.css";

export * from "./types";
export {
  BUILTIN_PALETTES,
  BUILTIN_READER_BACKGROUNDS,
  DEFAULT_APPEARANCE_CONFIG,
  applyAppearanceToDocument,
  getHito5Labels,
  getPaletteSpec,
  getReaderBackgroundClass,
} from "./palettes";
export type { Hito5Labels } from "./palettes";
export { createHito5Bridge, defaultHito5Bridge } from "./bridge";
export type { Hito5Bridge, TauriInvokeFn } from "./bridge";
export { sortExploreItems, sortLibraryItems } from "./sorting";
export {
  MangoraAppearanceProvider,
  useMangoraAppearance,
} from "./useMangoraAppearance";
export type {
  MangoraAppearanceContextValue,
  MangoraAppearanceProviderProps,
} from "./useMangoraAppearance";
export { useReaderMetrics, useReadingSessionTracker } from "./useReaderMetrics";
export type {
  UseReaderMetricsOptions,
  UseReadingSessionTrackerOptions,
} from "./useReaderMetrics";
export { AppearanceCustomizerPanel } from "./components/AppearanceCustomizerPanel";
export type { AppearanceCustomizerPanelProps } from "./components/AppearanceCustomizerPanel";
export { CollectionDisplayToolbar } from "./components/CollectionDisplayToolbar";
export type { CollectionDisplayToolbarProps } from "./components/CollectionDisplayToolbar";
export { ReaderMetricsDashboard } from "./components/ReaderMetricsDashboard";
export type { ReaderMetricsDashboardProps } from "./components/ReaderMetricsDashboard";
