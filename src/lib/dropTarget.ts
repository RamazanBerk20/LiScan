export type DropTarget = { scan: string } | { remote: string } | null;

/**
 * Reads the first entry of a dropped `text/uri-list`. Local `file:` URLs are
 * scanned; any other scheme only prefills the Remote dialog, so a dragged web
 * link never starts a network scan by itself.
 */
export function dropTarget(uriList: string): DropTarget {
  const line = uriList
    .split(/\r?\n/)
    .map((entry) => entry.trim())
    .find((entry) => entry && !entry.startsWith("#"));
  if (!line) return null;
  let url: URL;
  try {
    url = new URL(line);
  } catch {
    return null;
  }
  if (url.protocol !== "file:") return { remote: url.href };
  if (url.hostname && url.hostname !== "localhost") return null;
  try {
    return { scan: decodeURIComponent(url.pathname) };
  } catch {
    // Not UTF-8: the backend decodes the percent-encoded bytes itself.
    return { scan: `file://${url.pathname}` };
  }
}
