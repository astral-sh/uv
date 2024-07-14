from pandas import DataFrame

from pathlib import Path

# There isn't an indent, there is padding to {4,8,12}
padding_chars = " 0123456789"

# Parse the custom sizes format
lines = Path(__file__).parent.joinpath("sizes.txt").read_text().splitlines()
functions = {}
while True:
    [function_size, function_name] = lines.pop(0).removesuffix(" align=8").split(" ", 1)
    function_size = int(function_size)
    variants = []
    if not lines:
        break
    while len(lines[0]) - len(lines[0].lstrip(padding_chars)) >= 8:
        variant = lines.pop(0).strip().split(" ")
        variant_size = int(variant[0])
        if len(variant) == 3 and variant[2].startswith("Suspend"):
            inner = lines.pop(0).lstrip().split(" ")
            inner_size = int(inner[0])
            inner_name = inner[-1]
            variants.append((variant_size, inner_name, inner_size))
        elif len(variant) == 2 and variant[1].startswith("inner"):
            inner_size = variant_size
            inner_name = (
                function_name.split("<", 1)[1].rsplit(">", 1)[0]
                if "<" in function_name
                else function_name
            )
            variants.append((variant_size, inner_name, inner_size))
        else:
            variants.append((variant_size, None, None))
        while len(lines[0]) - len(lines[0].lstrip(padding_chars)) >= 12:
            lines.pop(0)
    assert lines.pop(0) == ""
    functions[function_name] = (function_size, variants)

# Idea 1: Find variants that are much larger than the other variant
overheads = []
for function_name, (function_size, variants) in functions.items():
    if len(variants) > 1:
        overhead = variants[0][0] - variants[1][0]
        if variants[0][1] and variants[1][1]:
            overheads.append((overhead, function_name, variants[0][1]))

df = DataFrame.from_records(overheads, columns=["overhead", "caller", "callee"])
highest_overhead = df.sort_values("overhead", ascending=False)[:20]
print(highest_overhead)
print(highest_overhead.to_csv("highest_overhead.csv", index=False))

# Idea 2: Find functions that are much larger than their variants
overheads2 = []
for function_name, (function_size, variants) in functions.items():
    if len(variants) > 0 and variants[0][2]:
        overhead = function_size - variants[0][2]
        if variants[0][1]:
            overheads2.append((overhead, function_name, variants[0][1]))

df = DataFrame.from_records(overheads2, columns=["overhead", "caller", "callee"])
highest_overhead = df.sort_values("overhead", ascending=False)[:20]
print(highest_overhead)
print(highest_overhead.to_csv("highest_overhead2.csv", index=False))

if __name__ == "__main__":
    pass
