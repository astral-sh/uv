#!/usr/bin/env python3

import os
import time
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from typing import BinaryIO
from urllib.parse import urlsplit

CHUNK_DELAY = float(os.environ.get("UV_ANTITHESIS_CHUNK_DELAY", "0.02"))
CHUNK_SIZE = int(os.environ.get("UV_ANTITHESIS_CHUNK_SIZE", str(64 * 1024)))


class StreamingRequestHandler(SimpleHTTPRequestHandler):
    def copyfile(self, source: BinaryIO, output: BinaryIO) -> None:
        if not urlsplit(self.path).path.startswith("/packages/"):
            super().copyfile(source, output)
            return

        while chunk := source.read(CHUNK_SIZE):
            output.write(chunk)
            output.flush()
            time.sleep(CHUNK_DELAY)


def main() -> None:
    handler = partial(StreamingRequestHandler, directory="/index")
    server = ThreadingHTTPServer(("0.0.0.0", 8000), handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
