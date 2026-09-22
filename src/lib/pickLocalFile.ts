import { open } from "@tauri-apps/plugin-dialog";

export async function pickLocalFile(title?: string): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    ...(title ? { title } : {}),
  });
  if (typeof selected !== "string") {
    return null;
  }
  const path = selected.trim();
  return path.length > 0 ? path : null;
}
