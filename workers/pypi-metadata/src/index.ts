import JSZip from "jszip";

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
      const query = url.search;

      if (path.startsWith("/packages/")) {
        // Given the path, extract `click-7.1.2`.
        const parts = path.split("/");
        const name = parts[parts.length - 1].split("-")[0];
        const version = parts[parts.length - 1].split("-")[1];

        // Extract the zip contents.
        // Now, fetch "https://files.pythonhosted.org/packages/d2/3d/fa76db83bf75c4f8d338c2fd15c8d33fdd7ad23a9b5e57eb6c5de26b430e/click-7.1.2-py2.py3-none-any.whl"
        response = await fetch(`https://files.pythonhosted.org${path}`, {
          cf: {
            // Always cache this fetch regardless of content type
            // for a max of 5 seconds before revalidating the resource
            cacheTtl: 5,
            cacheEverything: true,
          },
        });
        const buffer = await response.arrayBuffer();
        const archive = await JSZip.loadAsync(buffer);
        const file = await archive
          .folder(`${name}-${version}.dist-info`)
          ?.file("METADATA")
          ?.async("string");
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
      } else if (path.startsWith("/simple/")) {
        // Pass the request on to `https://pypi.org/`. Include query string.
        // Propagate headers.
        response = await fetch(`https://pypi.org${path}${query}`, {
          cf: {
            // Always cache this fetch regardless of content type
            // for a max of 5 seconds before revalidating the resource
            cacheTtl: 5,
            cacheEverything: true,
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
