# List all crates in the workspace in topological order.
#
# Crates are ordered such that dependencies come before dependents,
# ensuring that crates can be published in the order they are listed.
#
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///


import json
import subprocess


def topological_sort(workspace_members: list[str], packages: dict) -> list[str]:
    """Sort workspace members in topological order (dependencies first)."""
    # Build a map from package name to package ID for workspace members
    name_to_id = {
        packages[member_id]["name"]: member_id for member_id in workspace_members
    }
    id_to_name = {member_id: name for name, member_id in name_to_id.items()}

    # Build dependency graph: maps package ID to set of dependency IDs
    # Only include normal and build dependencies; ignore dev-dependencies
    deps_graph = {}

    for member_id in workspace_members:
        package = packages[member_id]
        deps = set()

        for dep in package.get("dependencies", []):
            # Filter out dev-dependencies (they don't block publishing)
            if dep.get("kind") == "dev":
                continue

            # dep.get("package") handles renamed dependencies (e.g., package = "real-name")
            # If not renamed, fall back to dep["name"]
            dep_name = dep.get("package", dep["name"])

            # Only include workspace dependencies
            if dep_name in name_to_id:
                deps.add(name_to_id[dep_name])

        deps_graph[member_id] = deps

    # Topological sort using Kahn's algorithm
    # in_degree[X] = number of workspace dependencies that X has
    in_degree = {pkg_id: len(deps_graph[pkg_id]) for pkg_id in workspace_members}

    # Start with crates that have no dependencies
    queue = sorted(
        (id_to_name[pkg_id], pkg_id)
        for pkg_id in workspace_members
        if in_degree[pkg_id] == 0
    )
    result = []

    while queue:
        _, pkg_id = queue.pop(0)
        result.append(pkg_id)

        # Find all crates that depend on this one and decrement their in-degree
        for other_id in workspace_members:
            if pkg_id in deps_graph[other_id]:
                in_degree[other_id] -= 1
                if in_degree[other_id] == 0:
                    # Insert in sorted position for deterministic output
                    queue.append((id_to_name[other_id], other_id))
                    queue.sort()

    # Check for cycles
    if len(result) != len(workspace_members):
        unresolved = set(workspace_members) - set(result)
        unresolved_names = sorted(id_to_name[pkg_id] for pkg_id in unresolved)
        raise ValueError(
            f"Circular dependency detected. Could not order: {', '.join(unresolved_names)}"
        )

    # Return crate names in topological order
    return [id_to_name[pkg_id] for pkg_id in result]


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}

    sorted_names = topological_sort(content["workspace_members"], packages)
    for name in sorted_names:
        print(name)


if __name__ == "__main__":
    main()
