
events = []
try:
    events.append("normal")
finally:
    events.append("cleanup-1")

try:
    raise ValueError("bad")
except ValueError:
    events.append("handled")
finally:
    events.append("cleanup-2")

def overriding_return():
    try:
        events.append("value")
        return len(events)
    finally:
        events.append("override")
        return 2

def break_from_finally():
    for value in [1, 2]:
        try:
            events.append(("try", value))
        finally:
            events.append(("break", value))
            break
    return "broken"

def continue_from_finally():
    result = []
    for value in [1, 2]:
        try:
            result.append(("try", value))
            raise ValueError(value)
        finally:
            result.append(("continue", value))
            continue
    return result

print(events)
print(overriding_return(), events)
print(break_from_finally(), events[-2:])
print(continue_from_finally())
