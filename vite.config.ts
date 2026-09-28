import path from "node:path";
import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },

  // 以下选项仅针对 `tauri dev` / `tauri build`
  // 1. 不清屏，避免遮住 Rust 的编译错误
  clearScreen: false,
  // 2. Tauri 需要固定端口，端口被占用时直接失败
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
      // 3. 不监听 src-tauri，Rust 代码由 Tauri 负责重新编译
      ignored: ["**/src-tauri/**"],
    },
  },
}));
