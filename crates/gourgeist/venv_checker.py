from pathlib import Path
from subprocess import check_output, check_call


def main():
    project_root = Path(__file__).parent
    venv_name = ".venv-rs"
    venv_python = f"{venv_name}/bin/python"
    venv_pip = f"{venv_name}/bin/pip"

    command = f". {venv_name}/bin/activate && which python"
    output = check_output(["bash"], input=command, text=True).strip()
    assert output == str(project_root.joinpath(venv_python)), output

    command = f". {venv_name}/bin/activate && wheel help"
    output = check_output(["bash"], input=command, text=True).strip()
    assert output.startswith("usage:"), output

    output = check_output([venv_python, "imasnake.py"], text=True).strip().splitlines()
    assert output[0] == str(project_root.joinpath(venv_python)), output
    assert not output[2].startswith(str(project_root)), output
    assert output[3] == str(project_root.joinpath(venv_name)), output

    check_call([venv_pip, "install", "tqdm"])


if __name__ == "__main__":
    main()
