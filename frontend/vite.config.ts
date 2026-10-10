import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,

  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ["**src-tauri/**"],
    },
  },
});
