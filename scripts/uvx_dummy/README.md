# uvx

`uvx` is provided by the [uv package](https://pypi.org/project/uv/). There is no need to install it
separately. This is just a dummy package guarding against dependendency confusion attacks.

Previously, this was a third-party package used to extend uv's functionality. The author of that
package graciously renamed it to [`uvenv`](https://pypi.org/project/uvenv/) to avoid confusion. If
you're attempting to use that package, replace your dependency on `uvx` with `uvenv`.
