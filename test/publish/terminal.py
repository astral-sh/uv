"""Run a publish command in a terminal, optionally answering its credential prompt."""

import argparse
import errno
import os
import pty
import select
import signal
import sys
import termios
import time

parser = argparse.ArgumentParser()
parser.add_argument("--username")
parser.add_argument("--password")
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()

pid, master = pty.fork()
if pid == 0:
    os.execvpe(args.command[0], args.command, os.environ)

output = bytearray()
username_sent = False
password_sent = False
status = None
try:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], 0.01)
        if ready:
            try:
                chunk = os.read(master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            output.extend(chunk)
            sys.stderr.buffer.write(chunk)
            sys.stderr.buffer.flush()

        if b"Enter username" in output and not username_sent:
            assert args.username is not None, "Unexpected username prompt"
            if not termios.tcgetattr(master)[3] & termios.ECHO:
                os.write(master, (args.username + "\n").encode())
                username_sent = True

        if b"Enter password:" in output and not password_sent:
            assert args.password is not None, "Unexpected password prompt"
            # The password reader flushes pending input as it disables terminal echo.
            if not termios.tcgetattr(master)[3] & termios.ECHO:
                os.write(master, (args.password + "\n").encode())
                password_sent = True
    else:
        raise TimeoutError("Publish did not finish within 30 seconds")

    _, status = os.waitpid(pid, 0)
finally:
    if status is None:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    os.close(master)

sys.exit(os.waitstatus_to_exitcode(status))
