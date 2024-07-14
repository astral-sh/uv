# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "pandas[output-formatting] >=2.2,<3",
# ]
# ///

import pandas

df = pandas.read_csv("scripts/stack-overflow/stacktrace.csv", index_col="id")
df["stack_pointer_above"] = df["stack_pointer"].shift(
    periods=1, fill_value=df["stack_pointer"].iloc[0]
)
df["frame_size"] = df["stack_pointer"] - df["stack_pointer_above"]
top_20 = df.sort_values("frame_size", ascending=False)[:20]
print(top_20[["frame_size", "name"]].to_markdown())
