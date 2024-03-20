import subprocess

if __name__ == "__main__":
    for i in range(50):
        subprocess.check_call(
            [
                "uv",
                "pip",
                "install",
                "-r",
                "requirements.txt",
                "--reinstall",
                "--no-cache",
            ]
        )
