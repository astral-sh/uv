from bird_feeder import use

try:
    from albatross import fly

    raise RuntimeError("albatross installed")
except ModuleNotFoundError:
    pass

print("Success")
