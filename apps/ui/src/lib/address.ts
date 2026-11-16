/** "localhost:3000" → "http://localhost:3000"; leaves complete addresses and empty input alone. */
export function withScheme(input: string): string {
  const raw = input.trim();
  return raw && !/^https?:\/\//.test(raw) ? `http://${raw}` : raw;
}

/** "http://localhost:3000/" → "localhost:3000" */
export function bareAddress(url: string): string {
  return url.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

/** Last part of a path, for naming things after their folder. */
export function folderName(path: string): string {
  return path.replace(/\/+$/, "").split("/").pop() ?? "";
}
