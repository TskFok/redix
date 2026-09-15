import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tauriConfig from "./src-tauri/tauri.conf.json";

// Desktop devUrl pages are served by Vite, so send the same development CSP here.
const devCsp = Object.entries(tauriConfig.app.security.devCsp)
  .map(([directive, sources]) => `${directive} ${sources}`)
  .join("; ");

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    headers: { "Content-Security-Policy": devCsp },
    watch: { ignored: ["**/src-tauri/**"] },
    hmr: { port: 1421 },
  },
});
