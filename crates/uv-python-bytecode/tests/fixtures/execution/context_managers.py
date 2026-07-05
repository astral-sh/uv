
events = []
class Manager:
    def __init__(self, suppress):
        self.suppress = suppress
    def __enter__(self):
        events.append("enter")
        return 42
    def __exit__(self, kind, value, traceback):
        events.append("exit")
        return self.suppress

with Manager(False) as value:
    events.append(value)

with Manager(True):
    raise ValueError("suppressed")

with Manager(False), Manager(True):
    raise ValueError("suppressed by inner manager")

for index in range(3):
    with Manager(False):
        events.append(index)
        if index == 0:
            continue
        break

for index in range(2):
    try:
        raise ValueError(index)
    except ValueError:
        events.append(f"handled-{index}")
        continue

print(events)
