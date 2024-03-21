import subprocess

if __name__ == "__main__":
    for i in range(50):
        subprocess.check_call(
            [
                "./uv",
                "pip",
                "sync",
                "requirements_all.txt",
                "--reinstall",
                "--no-cache",
                "--extra-index-url",
                "https://wheels.home-assistant.io/musllinux-index/"
            ]
        )
