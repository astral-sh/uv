from shared_lib import hello_shared
import numpy as np

def main():
    hello_shared()
    print(f"numpy {np.__version__}")

if __name__ == "__main__":
    main()
