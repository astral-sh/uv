This is a simple workspace with 2 applications `app1` and `app2` and 2 libraries.

The applications have conflicting dependencies:

- `app1` depends on `numpy == 2.0.0`
- `app2` depends on `numpy == 1.26.4`

Both `app1` and `app2` depend on another workspace member `shared_lib`, which in turn depends on
`shared_corelib`.

The workspace will create 3 `uv.lock` and `.venv`:

- in the root, containing `shared_lib` and `shared_corelib`
- in `app1`, with `app1`, `shared_lib`, `shared_corelib`
- in `app2`, with `app2`, `shared_lib`, `shared_corelib`
