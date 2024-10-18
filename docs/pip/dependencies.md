# 依存関係の宣言

依存関係を静的ファイルに宣言することは、アドホックなインストールで環境を変更する代わりに、ベストプラクティスです。依存関係が定義されたら、それらを[ロック](./compile.md)して、一貫性のある再現可能な環境を作成できます。

## `pyproject.toml` を使用する

`pyproject.toml` ファイルは、プロジェクトの設定を定義するためのPython標準です。

`pyproject.toml` ファイルにプロジェクトの依存関係を定義するには:

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
  "ruff>=0.3.0"
]
```

`pyproject.toml` ファイルにオプションの依存関係を定義するには:

```toml title="pyproject.toml"
[project.optional-dependencies]
cli = [
  "rich",
  "click",
]
```

各キーは「エクストラ」を定義しており、`--extra` および `--all-extras` フラグや `package[<extra>]` 構文を使用してインストールできます。詳細については、[パッケージのインストール](./packages.md#installing-packages-from-files)に関するドキュメントを参照してください。

`pyproject.toml` の使用を開始するための詳細については、公式の
[`pyproject.toml` ガイド](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/)を参照してください。

## `requirements.in` を使用する

プロジェクトの依存関係を宣言するために、軽量な `requirements.txt` 形式を使用することも一般的です。各要件は独自の行で定義されます。通常、このファイルは `requirements.txt` から区別するために `requirements.in` と呼ばれます。

`requirements.in` ファイルに依存関係を定義するには:

```python title="requirements.in"
httpx
ruff>=0.3.0
```

この形式ではオプションの依存関係グループはサポートされていません。
