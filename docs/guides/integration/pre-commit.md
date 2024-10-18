# Using uv in pre-commit

公式の pre-commit フックは [`astral-sh/uv-pre-commit`](https://github.com/astral-sh/uv-pre-commit) に提供されています。

pre-commit を介して requirements をコンパイルするには、次の内容を `.pre-commit-config.yaml` に追加します:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv バージョン。
  rev: 0.4.24
  hooks:
    # requirements をコンパイル
    - id: pip-compile
      args: [requirements.in, -o, requirements.txt]
```

別のファイルをコンパイルするには、`args` と `files` を変更します:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv バージョン。
  rev: 0.4.24
  hooks:
    # requirements をコンパイル
    - id: pip-compile
      args: [requirements-dev.in, -o, requirements-dev.txt]
      files: ^requirements-dev\.(in|txt)$
```

複数のファイルを同時にフックで処理するには:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv バージョン。
  rev: 0.4.24
  hooks:
    # requirements をコンパイル
    - id: pip-compile
      name: pip-compile requirements.in
      args: [requirements.in, -o, requirements.txt]
    - id: pip-compile
      name: pip-compile requirements-dev.in
      args: [requirements-dev.in, -o, requirements-dev.txt]
      files: ^requirements-dev\.(in|txt)$
```
