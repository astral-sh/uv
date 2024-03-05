This directory contains dockerfiles to reproduce build failures on package on linux when python was installed in different way on different platforms.

You can build and use them e.g. with

```
docker buildx build -f ubuntu-22-04-deadsnakes.dockerfile . -t ubuntu-22-04-deadsnakes --load
docker run --rm -it ubuntu-22-04-deadsnakes
```
