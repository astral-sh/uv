# Running scripts

## Including dependencies

When your script requires other packages, they must be installed into the environment that the
script runs in. uv prefers to create these environments on-demand instead of using a long-lived
virtual environment with manually managed dependencies. This requires explicit declaration of
dependencies that are required for the script. Generally, it's recommended to use a
[project](./projects.md) or [inline metadata](#declaring-script-dependencies) to declare
dependencies, but uv supports requesting dependencies per invocation as well.

For example, the following script requires `rich`.

```python title="example.py"
import time
from rich.progress import track

for i in track(range(20), description="For example:"):
    time.sleep(0.05)
```

If executed without specifying a dependency, this script will fail:

```console
$ uv run --no-project example.py
Traceback (most recent call last):
  File "/Users/astral/example.py", line 2, in <module>
    from rich.progress import track
ModuleNotFoundError: No module named 'rich'
```

Request the dependency using the `--with` option:

```console
$ uv run --with rich example.py
For example: ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 100% 0:00:01
```

Constraints can be added to the requested dependency if specific versions are needed:

```console
$ uv run --with 'rich>12,<13' example.py
```

Multiple dependencies can be requested by repeating with `--with` option.

Note that if `uv run` is used in a _project_, these dependencies will be included _in addition_ to
the project's dependencies. To opt-out of this behavior, use the `--no-project` flag.

## Switching Python versions

uv allows arbitrary Python versions to be requested on each script invocation, for example:

```python title="example.py"
import sys

print(".".join(map(str, sys.version_info[:3])))
```

```console
$ # Use the default Python version, may differ on your machine
$ uv run example.py
3.12.6
```

```console
$ # Use a specific Python version
$ uv run --python 3.10 example.py
3.10.15
```

See the [Python version request](../concepts/python-versions.md#requesting-a-version) documentation
for more details on requesting Python versions.

## Using GUI scripts

On Windows `uv` will run your script ending with `.pyw` extension using `pythonw`:

```python title="example.pyw"
from tkinter import Tk, ttk

root = Tk()
root.title("uv")
frm = ttk.Frame(root, padding=10)
frm.grid()
ttk.Label(frm, text="Hello World").grid(column=0, row=0)
root.mainloop()
```

```console
PS> uv run example.pyw
```

![Run Result](../assets/uv_gui_script_hello_world.png){: style="height:50px;width:150px"}

Similarly, it works with dependencies as well:

```python title="example_pyqt.pyw"
import sys
from PyQt5.QtWidgets import QApplication, QWidget, QLabel, QGridLayout

app = QApplication(sys.argv)
widget = QWidget()
grid = QGridLayout()

text_label = QLabel()
text_label.setText("Hello World!")
grid.addWidget(text_label)

widget.setLayout(grid)
widget.setGeometry(100, 100, 200, 50)
widget.setWindowTitle("uv")
widget.show()
sys.exit(app.exec_())
```

```console
PS> uv run --with PyQt5 example_pyqt.pyw
```

![Run Result](../assets/uv_gui_script_hello_world_pyqt.png){: style="height:50px;width:150px"}
