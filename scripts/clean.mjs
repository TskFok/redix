import { rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const projectRoot = new URL("../", import.meta.url);

for (const directory of ["node_modules/.vite", "dist", "src-tauri/target"]) {
  rmSync(fileURLToPath(new URL(directory, projectRoot)), {
    recursive: true,
    force: true,
  });
}
