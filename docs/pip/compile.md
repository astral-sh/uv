# 環境のロック

ロックとは、依存関係（例：`ruff`）を取得し、使用する正確なバージョンをファイルに書き込むことです。
多くの依存関係を扱う場合、正確なバージョンをロックすることで、環境を再現することができます。
ロックしない場合、依存関係のバージョンは時間の経過とともに、異なるツールを使用する場合やプラットフォーム間で変更される可能性があります。

## 依存関係のロック

uvは、依存関係を`requirements.txt`形式でロックすることができます。
依存関係を定義するために標準の`pyproject.toml`を使用することをお勧めしますが、他の依存関係形式もサポートされています。
依存関係の定義方法についての詳細は、[依存関係の宣言](dependencies.md)に関するドキュメントを参照してください。

`pyproject.toml`に宣言された依存関係をロックするには：

```console
$ uv pip compile pyproject.toml -o requirements.txt
```

デフォルトでは、`uv pip compile`の出力は表示されるだけであり、ファイルに書き込むには`--output-file` / `-o`引数が必要です。

`requirements.in`に宣言された依存関係をロックするには：

```console
$ uv pip compile requirements.in -o requirements.txt
```

複数のファイルに宣言された依存関係をロックするには：

```console
$ uv pip compile pyproject.toml requirements-dev.in -o requirements-dev.txt
```

uvは、従来の`setup.py`および`setup.cfg`形式もサポートしています。
`setup.py`に宣言された依存関係をロックするには：

```console
$ uv pip compile setup.py -o requirements.txt
```

標準入力から依存関係をロックするには、`-`を使用します：

```console
$ echo "ruff" | uv pip compile -
```

オプションの依存関係を有効にしてロックするには、例として「foo」エクストラを使用します：

```console
$ uv pip compile pyproject.toml --extra foo
```

すべてのオプションの依存関係を有効にしてロックするには：

```console
$ uv pip compile pyproject.toml --all-extras
```

`requirements.in`形式ではエクストラはサポートされていないことに注意してください。

## 依存関係のアップグレード

出力ファイルを使用する場合、uvは既存の出力ファイルに固定されたバージョンを考慮します。
依存関係が固定されている場合、後続のコンパイル実行時にアップグレードされません。
例：

```console
$ echo "ruff==0.3.0" > requirements.txt
$ echo "ruff" | uv pip compile - -o requirements.txt
# このファイルは次のコマンドによってuvによって自動生成されました：
#    uv pip compile - -o requirements.txt
ruff==0.3.0
```

依存関係をアップグレードするには、`--upgrade-package`フラグを使用します：

```console
$ uv pip compile - -o requirements.txt --upgrade-package ruff
```

すべての依存関係をアップグレードするには、`--upgrade`フラグがあります。

## 環境の同期

依存関係は、定義ファイルから直接、またはコンパイルされた`requirements.txt`ファイルから`uv pip install`を使用してインストールできます。
詳細については、[ファイルからのパッケージのインストール](packages.md#installing-packages-from-files)に関するドキュメントを参照してください。

`uv pip install`を使用してインストールする場合、既にインストールされているパッケージはロックファイルと競合しない限り削除されません。
これにより、環境にはロックファイルに宣言されていない依存関係が含まれる可能性があり、再現性に問題が生じることがあります。
環境がロックファイルと完全に一致することを確認するには、代わりに`uv pip sync`を使用します。

`requirements.txt`ファイルで環境を同期するには：

```console
$ uv pip sync requirements.txt
```

`pyproject.toml`ファイルで環境を同期するには：

```console
$ uv pip sync pyproject.toml
```

## 制約の追加

制約ファイルは、インストールされる要件の_バージョン_のみを制御する`requirements.txt`のようなファイルです。
ただし、制約ファイルにパッケージを含めても、そのパッケージのインストールはトリガーされません。
制約は、現在のプロジェクトの依存関係ではない依存関係に境界を追加するために使用できます。

制約を定義するには、パッケージの境界を定義します：

```python title="constraints.txt"
pydantic<2.0
```

制約ファイルを使用するには：

```console
$ uv pip compile requirements.in --constraint constraints.txt
```

各ファイルに複数の制約を定義でき、複数のファイルを使用できることに注意してください。

## 依存関係のバージョンの上書き

オーバーライドファイルは、`requirements.txt`のようなファイルで、構成パッケージによって宣言された要件に関係なく、特定のバージョンの要件を強制的にインストールします。
また、無効な解決と見なされる場合でも、インストールされます。

制約は_追加的_であり、構成パッケージの要件と組み合わされますが、オーバーライドは_絶対的_であり、構成パッケージの要件を完全に置き換えます。

オーバーライドは、推移的依存関係から上限を削除するために最も頻繁に使用されます。
たとえば、`a`が`c>=1.0,<2.0`を要求し、`b`が`c>=2.0`を要求し、現在のプロジェクトが`a`と`b`を要求する場合、依存関係は解決できません。

オーバーライドを定義するには、問題のあるパッケージの新しい要件を定義します：

```python title="overrides.txt"
c>=2.0
```

オーバーライドファイルを使用するには：

```console
$ uv pip compile requirements.in --override overrides.txt
```

これで、解決が成功します。
ただし、`a`が`c>=2.0`をサポートしていない場合、パッケージを使用する際にランタイムエラーが発生する可能性があることに注意してください。

各ファイルに複数のオーバーライドを定義でき、複数のファイルを使用できることに注意してください。
