from albatross import fly

try:
    from bird_feeder import use

    raise RuntimeError("bird-feeder installed")
except ModuleNotFoundError:
    pass

fly()
print("Success")
