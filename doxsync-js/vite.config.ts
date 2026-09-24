import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import dts from "vite-plugin-dts";

export default defineConfig({
  plugins: [dts({
    tsconfigPath: "./tsconfig.json",
    afterDiagnostic(diagnostics) {
      if (diagnostics.length) throw new Error("TypeScript declaration checks failed");
    },
  })],
  build: {
    target: "es2022",
    // The WASM build has already cleaned dist and populated dist/wasm.
    emptyOutDir: false,
    lib: {
      entry: {
        index: fileURLToPath(new URL("./src/index.ts", import.meta.url)),
        node: fileURLToPath(new URL("./src/node.ts", import.meta.url)),
      },
      formats: ["es"],
      fileName: (_format, entryName) => `${entryName}.js`,
    },
    rolldownOptions: {
      external: ["node:fs/promises", "./wasm/doxsync.js", "decimal.js"],
      output: {
        // Keep relative WASM imports next to the two public entry points.
        chunkFileNames: "[name]-[hash].js",
      },
    },
  },
});
