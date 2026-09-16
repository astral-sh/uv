# /// script
# requires-python = ">=3.12"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Exercise the supplemental Cargo bans against a local workspace."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

CHECKER = Path(__file__).with_name("check-cargo-bans.py")


class CargoDependencyChecks(unittest.TestCase):
    def setUp(self) -> None:
        directory = TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.root = self.directory / "workspace"

        self.write(
            "workspace/Cargo.toml",
            '[workspace]\nmembers = ["member"]\nresolver = "2"\n'
            "[workspace.dependencies]\n"
            'builder = { path = "../dependencies/builder-1.0.0", '
            "default-features = false }\n",
        )
        self.write(
            "workspace/member/Cargo.toml",
            '[package]\nname = "member"\nversion = "1.0.0"\nedition = "2021"\n'
            "[dependencies]\n"
            "builder = { workspace = true }\n"
            'optional = { path = "../../dependencies/optional-1.0.0", '
            "default-features = false, optional = true }\n"
            'previous = { package = "versioned", '
            'path = "../../dependencies/versioned-1.0.0", default-features = false }\n'
            'current = { package = "versioned", '
            'path = "../../dependencies/versioned-2.0.0", default-features = false }\n'
            'removed = { path = "../../dependencies/removed-1.0.0", '
            "default-features = false }\n"
            "[dev-dependencies]\n"
            'development = { path = "../../dependencies/development-1.0.0", '
            "default-features = false }\n"
            "[target.'cfg(target_os = \"none\")'.dependencies]\n"
            'platform = { path = "../../dependencies/platform-1.0.0", '
            "default-features = false }\n",
        )
        self.write("workspace/member/src/lib.rs", "")
        self.write("workspace/member/build.rs", "fn main() {}\n")
        self.write_crate(
            "builder",
            build_script=True,
            manifest="[features]\ndefault = []\n[dependencies]\n"
            'transitive = { path = "../transitive-1.0.0", default-features = false }\n',
        )
        for name in ("transitive", "optional", "development", "platform"):
            self.write_crate(name, build_script=True)
        self.write_crate("versioned", build_script=True)
        self.write_crate("versioned", version="2.0.0")
        self.write_crate("removed", build_script=True, package="build = false\n")

        (self.root / "scripts").mkdir()
        shutil.copyfile(CHECKER, self.root / "scripts" / CHECKER.name)
        self.allowed = [
            "builder",
            "development",
            "optional",
            "platform",
            "transitive",
            "versioned",
        ]

    def write(self, path: str, contents: str) -> None:
        destination = self.directory / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(contents)

    def write_crate(
        self,
        name: str,
        *,
        version: str = "1.0.0",
        build_script: bool = False,
        package: str = "",
        manifest: str = "",
    ) -> None:
        directory = f"dependencies/{name}-{version}"
        self.write(
            f"{directory}/Cargo.toml",
            f'[package]\nname = "{name}"\nversion = "{version}"\nedition = "2021"\n'
            f"{package}[workspace]\n{manifest}",
        )
        self.write(f"{directory}/src/lib.rs", "")
        if build_script:
            self.write(f"{directory}/build.rs", "fn main() {}\n")

    def check(self) -> subprocess.CompletedProcess[str]:
        self.write(
            "workspace/.cargo/deny.toml",
            f"[bans.build]\nallow-build-scripts = {json.dumps(self.allowed)}\n",
        )
        subprocess.run(
            ["cargo", "generate-lockfile", "--offline"],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        return subprocess.run(
            [sys.executable, str(self.root / "scripts" / CHECKER.name)],
            cwd=self.root,
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_current_allowlist(self) -> None:
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(
            result.stdout,
            "Workspace external dependencies request no default features.\n"
            "The cargo-deny build-script allowlist has no stale entries.\n",
        )

    def test_stale_allowlist(self) -> None:
        self.allowed.extend(["removed", "missing", "member", "missing"])
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(
            result.stderr,
            "error: .cargo/deny.toml: bans.build.allow-build-scripts entry 'member' "
            "does not match any dependency with a build script\n"
            "error: .cargo/deny.toml: bans.build.allow-build-scripts entry 'missing' "
            "does not match any dependency with a build script\n"
            "error: .cargo/deny.toml: bans.build.allow-build-scripts entry 'removed' "
            "does not match any dependency with a build script\n"
            "Remove stale entries from bans.build.allow-build-scripts.\n",
        )

    def test_default_features(self) -> None:
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text() + 'unused = "1.0"\n')
        manifest = self.root / "member" / "Cargo.toml"
        manifest.write_text(
            manifest.read_text()
            + '[features]\nexplicit-default = ["builder/default"]\n'
        )
        self.allowed.append("missing")
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(
            result.stderr,
            "error: Cargo.toml: workspace.dependencies.unused requests default features\n"
            f"error: {Path('member/Cargo.toml')}: features.explicit-default "
            "requests 'builder/default'\n"
            "Set default-features = false and request named features explicitly.\n"
            "error: .cargo/deny.toml: bans.build.allow-build-scripts entry 'missing' "
            "does not match any dependency with a build script\n"
            "Remove stale entries from bans.build.allow-build-scripts.\n",
        )


if __name__ == "__main__":
    unittest.main()
