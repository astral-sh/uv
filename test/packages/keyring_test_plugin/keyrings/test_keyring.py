import json
import os
import sys

from keyring import backend, credentials


class KeyringTest(backend.KeyringBackend):
    priority = 9

    def get_password(self, service, username):
        print(f"Keyring request for {username}@{service}", file=sys.stderr)
        entries = json.loads(os.environ.get("KEYRING_TEST_CREDENTIALS", "{}"))
        return entries.get(service, {}).get(username)

    def set_password(self, service, username, password):
        raise NotImplementedError()

    def delete_password(self, service, username):
        raise NotImplementedError()

    def get_credential(self, service, username):
        print(f"Keyring request for {service}", file=sys.stderr)
        entries = json.loads(os.environ.get("KEYRING_TEST_CREDENTIALS", "{}"))
        service_entries = entries.get(service, {})
        if not service_entries:
            return None
        if username:
            password = service_entries.get(username)
            if not password:
                return None
            return credentials.SimpleCredential(username, password)
        else:
            return credentials.SimpleCredential(*list(service_entries.items())[0])
