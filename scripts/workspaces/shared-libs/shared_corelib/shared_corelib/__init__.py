import tqdm

def hello_shared_core():
    print(f"shared-corelib is loaded from {__file__}")
    print(f"tqdm {tqdm.__version__}")
