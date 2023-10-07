import platform
from pathlib import Path
from subprocess import check_call, check_output


def check_installed(venv: Path) -> bool:
    """true: installed and working, false: not installed, borked: exception"""
    try:
        output = check_output(
            [
                venv.joinpath(
                    "Scripts" if platform.system() == "Windows" else "bin"
                ).joinpath("upsidedown")
            ],
            input="hello world!",
            text=True,
        ).strip()
    except FileNotFoundError:
        return False
    assert output == "¡pꞁɹoʍ oꞁꞁǝɥ"
    return True


def test_install_wheel_rs(pytestconfig, tmp_path):
    from install_wheel_rs import LockedVenv

    venv = tmp_path.joinpath("venv_test_install_wheel_rs")
    check_call(["virtualenv", venv])
    assert not check_installed(venv)
    locked_venv = LockedVenv(venv)
    wheel = pytestconfig.rootpath.joinpath(
        "install-wheel-rs/test-data/upsidedown-0.4-py2.py3-none-any.whl"
    )
    locked_venv.install_wheel(wheel)
    assert check_installed(venv)
