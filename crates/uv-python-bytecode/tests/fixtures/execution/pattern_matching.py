
class Point:
    __match_args__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y

def classify(value):
    match value:
        case None:
            return 'none'
        case (1 | 2) as number:
            return ('number', number)
        case {'x': x, **rest}:
            return ('mapping', x, rest)
        case [first, *middle, last] if first < last:
            return ('sequence', first, middle, last)
        case Point(x, y=0):
            return ('point', x)
        case _:
            return 'other'

print(classify(None))
print(classify(2))
print(classify({'x': 1, 'y': 2}))
print(classify([1, 2, 3, 4]))
print(classify(Point(5, 0)))
print(classify(object()))
