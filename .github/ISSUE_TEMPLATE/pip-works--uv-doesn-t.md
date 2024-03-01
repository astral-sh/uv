---
name: Pip works, uv doesn't
about: For cases where `pip install` works but `uv pip install` fails
title: ''
labels: ''
assignees: ''

---

**Description**
<!-- If you're using a non-standard python setup, please include some context on your setup. -->

**Requirements**
<!-- Please provide a list of requirements (requirements.in or requirements.txt), ideally a minimal set that fails. -->

```

```

**pip command and output**
<!-- The working pip command. Please make sure you are using `--no-cache-dir` to disable using cached built wheels. You can link long output as a [gist](https://gist.github.com/). -->

```shell
pip install --no-cache-dir <your_options_here>
```

**uv command**
<!-- The command you use to install with uv. Please make sure you are using `--no-cache`. -->

```shell
uv pip install --no-cache <your_options_here>
```

**Operating System and Python version**
OS: 
Python version:
How did you install python:
