import six
from shared_corelib import hello_shared_core

def hello_shared():
    print(f"shared-lib is loaded from {__file__}")
    print(f"six {six.__version__}")
    hello_shared_core()
