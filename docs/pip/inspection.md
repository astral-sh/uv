# Inspecting environments

## Listing installed packages

To list all of the packages in the environment:

```console
$ uv pip list
```

To list the packages in a JSON format:

```console
$ uv pip list --format json
```

To list all of the packages in the environment in a `requirements.txt` format:

```console
$ uv pip freeze
```

## Inspecting a package

To show information about an installed package, e.g., `numpy`:

```console
$ uv pip show numpy
```

Multiple packages can be inspected at once.

## Verifying an environment

It is possible to install packages with conflicting requirements into an environment if installed in
multiple steps.

To check for conflicts or missing dependencies in the environment:

```console
$ uv pip check
```
