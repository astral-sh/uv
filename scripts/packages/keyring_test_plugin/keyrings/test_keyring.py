import json
import os
import sys

from keyring import backend


class KeyringTest(backend.KeyringBackend):
    priority = 9

    def get_password(self, service, username):
        print(f"Request for {username}@{service}", file=sys.stderr)
        credentials = json.loads(os.environ.get("KEYRING_TEST_CREDENTIALS", "{}"))
        return credentials.get(service, {}).get(username)

    def set_password(self, service, username, password):
        raise NotImplementedError()

    def delete_password(self, service, username):
        raise NotImplementedError()

    def get_credential(self, service, username):
        raise NotImplementedError()
