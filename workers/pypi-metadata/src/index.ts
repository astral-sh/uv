import * as zip from "@zip.js/zip.js";

export interface Env {}

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext,
  ): Promise<Response> {
    const cacheUrl = new URL(request.url);

    // Construct the cache key from the cache URL
    const cacheKey = new Request(cacheUrl.toString(), request);
    const cache = caches.default;

    // Check whether the value is already available in the cache
    // if not, you will need to fetch it from origin, and store it in the cache
    let response = await cache.match(cacheKey);

    if (!response) {
      console.log(
        `Response for request url: ${request.url} not present in cache. Fetching and caching request.`,
      );

      // Take the path from the request. The path will be like:
      //   /packages/d2/3d/fa76db83bf75c4f8d338c2fd15c8d33fdd7ad23a9b5e57eb6c5de26b430e/click-7.1.2-py2.py3-none-any.whl
      const url = new URL(request.url);
      const path = url.pathname;

      if (path.startsWith("/packages/")) {
        // Given the path, extract `click-7.1.2`.
        const parts = path.split("/");
        const name = parts[parts.length - 1].split("-")[0];
        const version = parts[parts.length - 1].split("-")[1];

        // Read the metadata.
        const reader = new zip.ZipReader(
          new zip.HttpRangeReader(`https://files.pythonhosted.org${path}`),
        );
        const file = await readMetadata(reader, name, version);
        if (!file) {
          return new Response("Not found", { status: 404 });
        }

        // Return the metadata. Set immutable caching headers. Add content-length.
        response = new Response(file, {
          headers: {
            "Content-Type": "text/plain",
            "Content-Length": file.length.toString(),
            "Cache-Control": "public, max-age=31536000, immutable",
          },
        });

        ctx.waitUntil(cache.put(cacheKey, response.clone()));
      } else {
        return new Response("Not found", { status: 404 });
      }
    } else {
      console.log(`Cache hit for: ${request.url}.`);
    }

    return response;
  },
};

/**
 * Read the `METADATA` file from the given wheel.
 */
async function readMetadata(
  reader: zip.ZipReader<any>,
  name: string,
  version: string,
) {
  const entries = await reader.getEntriesGenerator();
  const target = `${name}-${version}.dist-info/METADATA`.toLowerCase();

  for await (const entry of entries) {
    // The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
    // the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
    // Either way, we just search the wheel for the name. See `find_dist_info`:
    // https://github.com/astral-sh/puffin/blob/2652caa3e31282afc2f1e1ca581ac4f553af710d/crates/install-wheel-rs/src/wheel.rs#L1024-L1057
    if (entry.filename.toLowerCase() == target) {
      return await entry.getData!(new zip.TextWriter());
    }
  }
  return null;
}
