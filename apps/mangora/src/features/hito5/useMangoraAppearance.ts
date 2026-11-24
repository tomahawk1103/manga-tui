import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { defaultHito5Bridge, type Hito5Bridge } from "./bridge";
import {
  BUILTIN_PALETTES,
  BUILTIN_READER_BACKGROUNDS,
  DEFAULT_APPEARANCE_CONFIG,
  applyAppearanceToDocument,
} from "./palettes";
import type {
  MangoraAppearanceConfig,
  PaletteSpec,
  ReaderBackgroundSpec,
} from "./types";

export interface MangoraAppearanceContextValue {
  config: MangoraAppearanceConfig;
  palettes: PaletteSpec[];
  readerBackgrounds: ReaderBackgroundSpec[];
  isLoaded: boolean;
  isSaving: boolean;
  saveError: string | null;
  updateAppearance: (patch: Partial<MangoraAppearanceConfig>) => Promise<void>;
  resetAppearance: () => Promise<void>;
}

const MangoraAppearanceContext = createContext<MangoraAppearanceContextValue | null>(null);

export interface MangoraAppearanceProviderProps {
  children: React.ReactNode;
  bridge?: Hito5Bridge;
  targetElement?: HTMLElement | null;
}

export function MangoraAppearanceProvider({
  children,
  bridge = defaultHito5Bridge,
  targetElement,
}: MangoraAppearanceProviderProps) {
  const [config, setConfig] = useState<MangoraAppearanceConfig>(DEFAULT_APPEARANCE_CONFIG);
  const [palettes, setPalettes] = useState<PaletteSpec[]>(BUILTIN_PALETTES);
  const [readerBackgrounds, setReaderBackgrounds] = useState<ReaderBackgroundSpec[]>(
    BUILTIN_READER_BACKGROUNDS,
  );
  const [isLoaded, setIsLoaded] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const configRef = useRef<MangoraAppearanceConfig>(DEFAULT_APPEARANCE_CONFIG);

  // Keep DOM tokens synchronized live on every update
  useEffect(() => {
    configRef.current = config;
    applyAppearanceToDocument(config, targetElement, palettes);
  }, [config, palettes, targetElement]);

  // Initial load from config.toml (via Tauri) or local fallback
  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const [catalog, loaded] = await Promise.all([
          bridge.getAppearanceCatalog(),
          bridge.getAppearanceConfig(),
        ]);
        if (!active) return;
        if (catalog.palettes.length > 0) setPalettes(catalog.palettes);
        if (catalog.reader_backgrounds.length > 0) {
          setReaderBackgrounds(catalog.reader_backgrounds);
        }
        setConfig(loaded);
        configRef.current = loaded;
        applyAppearanceToDocument(loaded, targetElement, catalog.palettes);
      } catch (err) {
        if (!active) return;
        setSaveError(err instanceof Error ? err.message : String(err));
      } finally {
        if (active) setIsLoaded(true);
      }
    })();
    return () => {
      active = false;
    };
  }, [bridge, targetElement]);

  const updateAppearance = useCallback(
    async (patch: Partial<MangoraAppearanceConfig>) => {
      const next: MangoraAppearanceConfig = { ...configRef.current, ...patch };
      configRef.current = next;
      setConfig(next);
      setIsSaving(true);
      setSaveError(null);
      try {
        const saved = await bridge.saveAppearanceConfig(next);
        configRef.current = saved;
        setConfig(saved);
      } catch (err) {
        setSaveError(err instanceof Error ? err.message : String(err));
      } finally {
        setIsSaving(false);
      }
    },
    [bridge],
  );

  const resetAppearance = useCallback(async () => {
    await updateAppearance(DEFAULT_APPEARANCE_CONFIG);
  }, [updateAppearance]);

  const value = useMemo<MangoraAppearanceContextValue>(
    () => ({
      config,
      palettes,
      readerBackgrounds,
      isLoaded,
      isSaving,
      saveError,
      updateAppearance,
      resetAppearance,
    }),
    [config, palettes, readerBackgrounds, isLoaded, isSaving, saveError, updateAppearance, resetAppearance],
  );

  return React.createElement(MangoraAppearanceContext.Provider, { value }, children);
}

/**
 * Hook for reading and updating Mangora visual customization live.
 */
export function useMangoraAppearance(): MangoraAppearanceContextValue {
  const ctx = useContext(MangoraAppearanceContext);
  if (!ctx) {
    throw new Error("useMangoraAppearance must be used within a MangoraAppearanceProvider");
  }
  return ctx;
}
