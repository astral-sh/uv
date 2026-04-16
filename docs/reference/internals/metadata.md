# Workspace metadata

`uv workspace metadata` exports the information uv has about your workspace as JSON so other tools
can use it. In particular, if you want access to the information in a `uv.lock`, you should prefer
this command's output, as `uv.lock` is not a stable format we guarantee anything about.

The primary structure is the "resolution" field which contains the dependency graph with exact
package versions that a `uv.lock` encodes.

The edges of the graph are the `dependencies` every node defines. These are the things that must
also be installed for it to be installed (and their `dependencies` recursively, keeping in mind that
cycles are perfectly normal to encounter in this graph). Each dependency entry will include an `id`
for the node it refers to, and an optional `marker` that
[specifies on what platforms the dependency is required](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#dependency-specifiers)
(if there is no marker the dependency is always required).

Nodes in the graph are uniquely identified by package `name`, `version`, `source`, and `kind`.

There are 3 kinds of node in the graph:

- `"package"` -- the package itself
- `{ "extra": "extraname" }` -- an extra the package defines
- `{ "group": "groupname" }` -- a dependency group the package defines

(In the future we will add "build" nodes for the dependencies of
[build environments](https://docs.astral.sh/uv/concepts/projects/config/#build-isolation).)

If you want to install `mypackage`, find its `"kind": "package"` node. This node will also include
information on its sdist, its wheels, its extras (`optional_dependencies`), and dependency groups
(`dependency_groups`).

If you want to install `mypackage[myextra]` then find the node with `"kind": { "extra": "myextra" }`
for `mypackage` (this node will always depend on `mypackage`). If you want to install
`mypackage[extra1, extra2]`, find the two nodes for `mypackage[extra1]` and `mypackage[extra2]`.

If you want to install the dependency group `mypackage:mygroup` then find the node with
`"kind": { "group": "mygroup" }` for `mypackage` (this node will _not_ depend on `mypackage`, as
dependency groups are just lists of things you might want when working on the package itself).

## Handling multiple versions of a package

Two versions of a package cannot be installed into a python environment, but the dependency graph
may still include multiple versions of a package. This can happen for two different reasons.

The first way is for
[different platforms](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#dependency-specifiers)
to have conflicting requirements that force different versions of a package to be used.

The second way is when a workspace has
[conflicts](https://docs.astral.sh/uv/concepts/resolution/#conflicting-dependencies), implying some
workspace members or their extras are mutually exclusive, and only one of them can be installed at a
time. Information about conflicts can be found in the top-level `conflicts` field.

The specific guarantee we provide is that **for any concrete choice of
[markers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#dependency-specifiers),
if you select a set of packages to install that has no
[conflicts](https://docs.astral.sh/uv/concepts/resolution/#conflicting-dependencies), then the
resulting set of packages to install will not have multiple versions of a package**.

If you just want to get "every version of pydantic this workspace uses" you're free to iterate
through the list of nodes and collect up every instance. If however you want to specifically analyze
the graph and get actual resolutions you will likely need to consult `conflicts` and need to
understand how to resolve `markers` for a specific platform.

The best way to avoid mistakes when working with multiple versions of a package is to keep your
queries into the dependency graph rooted in operations on workspace members, as those are the
natural entry-points to the graph that uv wants to work on, and can give coherent responses for:
"install `member1` and `member2[extra]`".

Another way to put this is that when possible _you should avoid iterating over the `resolution`
object to find a node_. Only access `resolution` like a map using ids that were provided by another
part of the metadata. The only ids this initially gives you access to are the ones listed in the
`members` array, which lists all the workspace members. From there you may find the ids of that
package's dependencies, extras, and dependency groups and recursively discover other packages.

So rather than trying to find a node for anyio in the dependency graph directly, you should decide
what workspace member(s) you're interested in analyzing as if they were going to be installed. While
traversing the `dependencies` of the things you want to install, you may visit an instance of anyio,
which is the one you should use. If you visit multiple instances of anyio then that means you've
selected a conflicting set of things to install which uv would never select.

So if you wanted to analyze say, installing the `dev` dependency group of the workspace member
`mypackage` it would look something like:

```python
member = find_by_name(metadata.members, "mypackage")
member_node = metadata.resolution[member.id]
group = find_by_name(member_node.dependency_groups, "dev")
group_node = metadata.resolution[group.id]
visit(metadata, [group_node])
```

If you wanted to analyze two particular workspace members installed together, it would look
something like:

```python
to_analyze = []
for member_name in ["package1", "package2"]:
  member = find_by_name(metadata.members, member_name)
  member_node = metadata.resolution[member.id]
  to_analyze.append(member_node)
visit(metadata, to_analyze)
```

Where `visit` is your favourite graph traversal algorithm like depth-first-search:

```python
def visit(metadata: UvMetadata, to_analyze: list[Node]):
  visited = set()
  while len(to_analyze) > 0:
    node = to_analyze.pop()

    # Handle cycles by avoiding revisiting nodes
    if node.id in visited:
      continue
    visited.add(node.id)

    # We also need to analyze its dependencies
    for dependency in node.dependencies:
      # Only follow edges if they satisfy the desired platform's markers
      if dependency.marker and not satisfies(platform, dependency.marker):
        continue
      to_analyze.append(metadata.resolution[dependency.id])

    # Analyze any package node we encounter
    if node.kind == "package":
      print(node.name, node.version, node.source)
```

## Schema

A full JSON schema for the format will be provided when the format is finalized.

Here is a human-readable annotated example:

```js
{
	// Information about the schema of this output
	"schema": {
		// The version of this output, currently "preview"
		"version": "preview"
	},
	// The directory the uv.lock can be found in
	"workspace_root": "/workspace",
	// Any requirements on the python version this workspace has
  //
  // `marker` fields all have this as an implicit constraint that is omitted for cleanliness
	"requires_python": ">=3.12",
	// A list of workspace members
	"members": [
    {
      // The name of the package
      "name": "mypackage",
      // The directory that contains its pyproject.toml
      "path": "/workspace/packages/mypackage",
      // The id of this package's info in the `resolution` map below
      "id": "mypackage==0.1.0@editable+/workspace/packages/mypackage"
    },
	],
  // A list-of-sets of workspace items that are mutually-exclusive to install,
  // presumably because they need to install different versions of the same package.
  //
  // Any attempt to install two things that belong to the same set must be rejected.
  //
  // There are 3 kinds of item:
  //
  // * Project -- "kind": "project"
  // * Extra   -- "kind": { "extra": "extraname" }
  // * Group   -- "kind": { "group": "groupname" }
  "conflicts": {
    "sets": [
      {
        "items": [
          {
            "package": "mypackage",
            "kind": { "extra": "myextra" }
            "id": "mypackage[myextra]==0.1.0@editable+/workspace/packages/mypackage",
          }
          {
            "package": "mypackage",
            "kind": { "group": "mygroup" }
            "id": "mypackage:mygroup==0.1.0@editable+/workspace/packages/mypackage",
          }
        ]
      }
    ]
  }
  // Resolved information about packages and dependencies.
  //
  // Each entry in this map is a node in the dependency graph. There are currently
  // 3 kinds of node in the dependency graph, although more are planned in the future.
  //
  // * Packages -- "kind": "package"
  // * Extras   -- "kind": { "extra": "extraname" }
  // * Groups   -- "kind": { "group": "groupname" }
  //
  // Package nodes contain most of the metadata, while other nodes are mostly just a list
  // of dependencies. The different kinds of node are included like this to encourage correct
  // analysis of the graph. For instance, a node for `mypackage[someextra]` always depends on
  // `mypackage`, while `mypackage:somegroup` does not (because dependency-groups are just a
  // list of packages you might want to install while working on `mypackage`). Sugars like
  // `mypackage[extra1, extra2]` are decomposed into separate dependencies on `mypackage[extra1]`
  // and `mypackage[extra2]`.
  //
  // The ids used here are human-readable but should be handled as opaque (the nodes contain
  // the same information in a more convenient form).
  "resolution": {

    // This node is a workspace member
    "mypackage==0.1.0@editable+/workspace/packages/mypackage": {
      // The name of the package
      "name": "mypackage",
      // The version of the package (this may be missing, as source trees do not need versions)
      "version": "0.1.0",
      // The source of the package, in this case it's an editable whose path relative to the
      // `workspace_root` is `./packages/mypackage`
      "source": {
        "editable": "/workspace/packages/mypackage"
      },
      // The kind of the node, in this case "package" (see the docs on `resolution` above for details)
      "kind": "package",
      // The dependencies that must be installed to also install this node into an environment
      "dependencies": [
        {
          // The id of the node to lookup for details
          "id": "iniconfig==2.0.0@registry+https://pypi.org/simple"
          "marker": "marker": "sys_platform == 'linux'"
        }
      ],
      // The extras that this package defines
      "optional_dependencies": [
        {
          "name": "myextra",
          "id": "mypackage[myextra]==0.1.0@editable+/workspace/packages/mypackage"
        }
      ],
      // The dependency groups this package defines
      "dependency_groups": [
        {
          "name": "mygroup",
          "id": "mypackage:mygroup==0.1.0@editable+/workspace/packages/mypackage"
        }
      ]
    },

    // This node is an extra on a workspace member
    "mypackage[myextra]==0.1.0@editable+/workspace/packages/mypackage": {
      // These fields will match the package node above
      "name": "mypackage",
      "version": "0.1.0",
      "source": {
        "editable": "/workspace/packages/mypackage"
      },
      // But these two will differ from the package node above
      "kind": { "extra": "myextra" },
      "dependencies": [
        {
          "id": "mypackage==0.1.0@editable+/workspace/packages/mypackage"
        }
        {
          "id": "anyio==2.0.0@registry+https://pypi.org/simple"
        }
      ]
    },

    // This node is a dependency-group on a workspace member
    "mypackage:mygroup==0.1.0@editable+/workspace/packages/mypackage": {
      // These fields will match the package node above
      "name": "mypackage",
      "version": "0.1.0",
      "source": {
        "editable": "/workspace/packages/mypackage"
      },
      // But these two will differ from the package node above
      "kind": { "extra": "myextra" },
      "dependencies": [
        {
          "id": "anyio==1.0.0@registry+https://pypi.org/simple"
        }
      ]
    },

    // This node is a package on pypi
    "iniconfig==2.0.0@registry+https://pypi.org/simple": {
      "name": "iniconfig",
      "version": "2.0.0",
      // registry sources look like this
      "source": {
        "registry": {
          "url": "https://pypi.org/simple"
        }
      },
      "kind": "package",
      "dependencies": [],
      // Details on the package's source distribution
      "sdist": {
        // May alternatively be `path`
        "url": "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
        "hashes": {
          "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
        },
        "size": 4646,
        "upload_time": "2023-01-07T11:08:11.254Z"
      },
      // The wheels we found for this package
      "wheels": [
        {
          // May alternatively be `path`
          "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
          "hashes": {
            "sha256": "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
          },
          "size": 5892,
          "upload_time": "2023-01-07T11:08:09.864Z",
          // Parsing this name is how you know what platform a wheel supports
          "filename": "iniconfig-2.0.0-py3-none-any.whl"
        }
      ]
    }

    // ...and so on
    "anyio==1.0.0@registry+https://pypi.org/simple": { ... }
    "anyio==2.0.0@registry+https://pypi.org/simple": { ... }
  }
}
```
