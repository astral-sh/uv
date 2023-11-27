#!/usr/bin/env python3
"""
Compare puffin's resolution with pip-compile on a number of requirement sets and python
versions.

If the first resolution diverged, we run a second "coerced" try in which puffin gets the
output of pip as additional input to check if it considers this resolution possible.
"""
import json
import os
import subprocess
import time
from argparse import ArgumentParser
from hashlib import sha256
from pathlib import Path
from subprocess import check_output, check_call, CalledProcessError

default_targets = [
    "pandas",
    "pandas==2.1",
    "black[d,jupyter]",
    "meine_stadt_transparent",
    "jupyter",
    "transformers[tensorboard]",
    "transformers[accelerate,agents,audio,codecarbon,deepspeed,deepspeed-testing,dev,dev-tensorflow,dev-torch,flax,flax-speech,ftfy,integrations,ja,modelcreation,onnx,onnxruntime,optuna,quality,ray,retrieval,sagemaker,sentencepiece,sigopt,sklearn,speech,testing,tf,tf-cpu,tf-speech,timm,tokenizers,torch,torch-speech,torch-vision,torchhub,video,vision]",
]

data_root = Path(__file__).parent
project_root = Path(
    check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
)


def resolve_pip(targets: list[str], pip_compile: Path) -> list[str]:
    output = check_output(
        [
            pip_compile,
            "--allow-unsafe",
            "--strip-extras",
            "--upgrade",
            "--output-file",
            "-",
            "--quiet",
            "-",
        ],
        input=" ".join(targets),
        stderr=subprocess.STDOUT,
        text=True,
    )
    pip_deps = []
    for line in output.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        pip_deps.append(line)
    pip_deps.sort()
    return pip_deps


def resolve_puffin(targets: list[str], venv: Path, profile: str = "dev") -> list[str]:
    target_profile = profile if profile != "dev" else "debug"
    output = check_output(
        [
            project_root.joinpath("target").joinpath(target_profile).joinpath("puffin-dev"),
            "resolve-cli",
            "--format",
            "expanded",
            *targets,
        ],
        text=True,
        stderr=subprocess.STDOUT,
        env={
            **os.environ,
            "VIRTUAL_ENV": venv,
        },
    )
    puffin_deps = []
    for line in output.splitlines():
        puffin_deps.append(line.replace(" ", ""))
    puffin_deps.sort()
    return puffin_deps


def compare_for_python_version(
    python_major: int, python_minor: int, targets: list[str], profile: str = "dev"
):
    venvs = data_root.joinpath("venvs")
    venvs.mkdir(exist_ok=True)
    venvs.joinpath(".gitignore").write_text("*")
    cache = data_root.joinpath("pip_compile_cache")
    cache.mkdir(exist_ok=True)
    cache.joinpath(".gitignore").write_text("*")
    pip_compile_venv = venvs.joinpath(f"pip_compile_py{python_major}{python_minor}")
    if not pip_compile_venv.is_dir():
        check_call(
            ["virtualenv", "-p", f"{python_major}.{python_minor}", pip_compile_venv]
        )
        check_call(
            [pip_compile_venv.joinpath("bin").joinpath("pip"), "install", "pip-tools"]
        )
    pip_compile = pip_compile_venv.joinpath("bin").joinpath("pip-compile")
    for target in targets:
        digest = (
            f"py{python_major}{python_minor}-"
            + sha256(str(target).encode()).hexdigest()
        )
        cache_file = cache.joinpath(digest).with_suffix(".json")
        if cache_file.is_file():
            pip_result = json.loads(cache_file.read_text())
            pip_time = 0.0
        else:
            start = time.time()
            try:
                pip_result = resolve_pip([target], pip_compile)
                cache_file.write_text(json.dumps(pip_result))
            except CalledProcessError as e:
                pip_result = e
            pip_time = time.time() - start

        start = time.time()
        try:
            puffin_result = resolve_puffin([target], pip_compile_venv, profile=profile)
        except CalledProcessError as e:
            puffin_result = e
        puffin_time = time.time() - start

        if isinstance(pip_result, CalledProcessError) and isinstance(
            puffin_result, CalledProcessError
        ):
            print(f"Both failed {python_major}.{python_minor} {target}")
            continue
        elif isinstance(pip_result, CalledProcessError):
            # Make the output a bit more readable
            output = "\n".join(pip_result.output.splitlines()[:10])
            print(
                f"Only pip failed {python_major}.{python_minor} {target}: "
                f"{pip_result}\n---\n{output}\n---"
            )
            continue
        elif isinstance(puffin_result, CalledProcessError):
            # Make the output a bit more readable
            output = "\n".join(puffin_result.output.splitlines()[:10])
            print(
                f"Only puffin failed {python_major}.{python_minor} {target}: "
                f"{puffin_result}\n---\n{output}\n---"
            )
            continue

        if pip_result != puffin_result and isinstance(pip_result, list):
            # Maybe, both resolution are allowed? By adding all constraints from the pip
            # resolution we check whether puffin considers this resolution possible
            # (vs. there is a bug in puffin where we wouldn't pick those versions)
            start = time.time()
            try:
                puffin_result2 = resolve_puffin(
                    [target, *pip_result], pip_compile_venv, profile=profile
                )
            except CalledProcessError as e:
                puffin_result2 = e
            puffin_time2 = time.time() - start
            if puffin_result2 == pip_result:
                print(
                    f"Equal (coerced) {python_major}.{python_minor} "
                    f"(pip: {pip_time:.3}s, puffin: {puffin_time2:.3}s) {target}"
                )
                continue

        if pip_result == puffin_result:
            print(
                f"Equal {python_major}.{python_minor} "
                f"(pip: {pip_time:.3}s, puffin: {puffin_time:.3}s) {target}"
            )
        else:
            print(
                f"Different {python_major}.{python_minor} "
                f"(pip: {pip_time:.3}s, puffin: {puffin_time:.3}s) {target}"
            )
            print(f"pip: {pip_result}")
            print(f"puffin: {puffin_result}")
            while True:
                if pip_result and puffin_result:
                    if pip_result[0] == puffin_result[0]:
                        pip_result.pop(0)
                        puffin_result.pop(0)
                    elif pip_result[0] < puffin_result[0]:
                        print(f"- {pip_result.pop(0)}")
                    else:
                        print(f"+ {puffin_result.pop(0)}")
                elif pip_result:
                    print(f"- {pip_result.pop(0)}")
                elif puffin_result:
                    print(f"+ {puffin_result.pop(0)}")
                else:
                    break


def main():
    parser = ArgumentParser()
    parser.add_argument("--target", help="A list of requirements")
    parser.add_argument("-p", "--python")
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()

    if args.target:
        targets = [args.target]
    else:
        targets = default_targets

    if args.release:
        profile = "release"
    else:
        profile = "dev"

    check_call(["cargo", "build", "--bin", "puffin-dev", "--profile", profile])

    if args.python:
        python_major = int(args.python.split(".")[0])
        python_minor = int(args.python.split(".")[1])

        assert python_major == 3
        assert python_minor >= 8
        compare_for_python_version(python_major, python_minor, targets, profile=profile)
    else:
        for python_minor in range(8, 12):
            compare_for_python_version(3, python_minor, targets, profile=profile)


if __name__ == "__main__":
    main()
