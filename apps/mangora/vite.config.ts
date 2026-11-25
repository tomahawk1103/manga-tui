import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { mangaOniPreview } from "./web/mangaoni";

const host = (
  globalThis as { process?: { env?: Record<string, string | undefined> } }
).process?.env?.TAURI_DEV_HOST;

export default defineConfig(({ mode }) => ({
  ...((mode === "web" || mode === "mangaoni") && { root: "web", base: "./" }),
  plugins: [react(), tailwindcss(), mode === "mangaoni" && mangaOniPreview()],
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
