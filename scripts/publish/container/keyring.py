"""Exercise the real keyring CLI with only its dedicated test credential."""

import sys

import keyring
from keyring.cli import main

# The store lives in the container's temporary HOME, never on the host. Passing the
# token on stdin also keeps it out of Docker's command line and container config.
keyring.set_password(
    "https://test.pypi.org/legacy/?astral-test-keyring", "__token__", sys.stdin.read()
)
sys.exit(main())
