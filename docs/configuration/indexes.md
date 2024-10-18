# パッケージインデックス

デフォルトでは、uvは依存関係の解決とパッケージのインストールに[Python Package Index (PyPI)](https://pypi.org)を使用します。ただし、uvは`[[tool.uv.index]]`構成オプション（およびコマンドラインオプションの`--index`）を介して、プライベートインデックスを含む他のパッケージインデックスを使用するように構成できます。

## インデックスの定義

依存関係を解決する際に追加のインデックスを含めるには、`pyproject.toml`に`[[tool.uv.index]]`エントリを追加します：

```toml
[[tool.uv.index]]
# インデックスのオプションの名前。
name = "pytorch"
# インデックスの必須URL。
url = "https://download.pytorch.org/whl/cpu"
```

インデックスは定義された順序で優先され、構成ファイルに最初にリストされたインデックスが依存関係を解決する際に最初に参照され、コマンドラインで提供されたインデックスは構成ファイル内のインデックスよりも優先されます。

デフォルトでは、uvはPython Package Index (PyPI)を「デフォルト」インデックスとして含めます。これは、他のインデックスにパッケージが見つからない場合に使用されるインデックスです。PyPIをインデックスのリストから除外するには、別のインデックスエントリに`default = true`を設定します（またはコマンドラインオプションの`--default-index`を使用します）：

```toml
[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
default = true
```

デフォルトのインデックスは、リスト内の位置に関係なく、常に最も低い優先度として扱われます。

## パッケージをインデックスに固定する

パッケージは、`tool.uv.sources`エントリでインデックスを指定することにより、特定のインデックスに固定できます。たとえば、`torch`が常に`pytorch`インデックスからインストールされるようにするには、次の内容を`pyproject.toml`に追加します：

```toml
[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
```

同様に、プラットフォームに基づいて異なるインデックスから取得するには、環境マーカーで区別されたソースのリストを提供できます：

```toml title="pyproject.toml"
[project]
dependencies = ["torch"]

[tool.uv.sources]
torch = [
  { index = "pytorch-cu118", marker = "sys_platform == 'darwin'"},
  { index = "pytorch-cu124", marker = "sys_platform != 'darwin'"},
]

[[tool.uv.index]]
name = "pytorch-cu118"
url = "https://download.pytorch.org/whl/cu118"

[[tool.uv.index]]
name = "pytorch-cu124"
url = "https://download.pytorch.org/whl/cu124"
```

パッケージがそのインデックスに明示的に固定されていない限り、そのインデックスからパッケージがインストールされないようにするには、インデックスを`explicit = true`としてマークできます。たとえば、`torch`が`pytorch`インデックスからインストールされることを保証し、他のすべてのパッケージがPyPIからインストールされるようにするには、次の内容を`pyproject.toml`に追加します：

```toml
[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
explicit = true
```

`tool.uv.sources`を介して参照される名前付きインデックスは、プロジェクトの`pyproject.toml`ファイル内で定義されている必要があります。コマンドライン、環境変数、またはユーザーレベルの構成を介して提供されたインデックスは認識されません。

## 複数のインデックスを横断して検索する

デフォルトでは、uvは指定されたパッケージが利用可能な最初のインデックスで停止し、その最初のインデックスに存在する解決策に制限します（`first-match`）。

たとえば、`[[tool.uv.index]]`を介して内部インデックスが指定されている場合、uvの動作は、パッケージがその内部インデックスに存在する場合、常にその内部インデックスからインストールされ、PyPIからはインストールされないというものです。これは、攻撃者が内部パッケージと同じ名前の悪意のあるパッケージをPyPIに公開し、その結果、内部パッケージの代わりに悪意のあるパッケージがインストールされる「依存関係の混乱」攻撃を防ぐことを目的としています。たとえば、2022年12月の[torchtriton攻撃](https://pytorch.org/blog/compromised-nightly-dependency/)を参照してください。

ユーザーは、`--index-strategy`コマンドラインオプションまたは`UV_INDEX_STRATEGY`環境変数を介して、代替のインデックス動作を選択できます。これには次の値がサポートされています：

- `first-match`（デフォルト）：すべてのインデックスで各パッケージを検索し、パッケージを含む最初のインデックスに存在する候補バージョンに制限します。
- `unsafe-first-match`：すべてのインデックスで各パッケージを検索しますが、他のインデックスに新しいバージョンが存在する場合でも、互換性のあるバージョンを含む最初のインデックスを優先します。
- `unsafe-best-match`：すべてのインデックスで各パッケージを検索し、候補バージョンの組み合わせセットから最適なバージョンを選択します。

`unsafe-best-match`はpipの動作に最も近いですが、「依存関係の混乱」攻撃のリスクにさらされます。

## 資格情報の提供

ほとんどのプライベートレジストリは、パッケージにアクセスするために認証を必要とし、通常はユーザー名とパスワード（またはアクセストークン）を使用します。

プライベートインデックスで認証するには、環境変数を介して資格情報を提供するか、URLに埋め込みます。

たとえば、ユーザー名（`public`）とパスワード（`koala`）を必要とする`internal`という名前のインデックスがある場合、資格情報なしでインデックスを`pyproject.toml`に定義します：

```toml
[[tool.uv.index]]
name = "internal"
url = "https://example.com/simple"
```

その後、`INTERNAL`がインデックス名の大文字バージョンである`UV_INDEX_INTERNAL_USERNAME`および`UV_INDEX_INTERNAL_PASSWORD`環境変数を設定できます：

```sh
export UV_INDEX_INTERNAL_USERNAME=public
export UV_INDEX_INTERNAL_PASSWORD=koala
```

環境変数を介して資格情報を提供することで、プレーンテキストの`pyproject.toml`ファイルに機密情報を保存することを避けることができます。

または、資格情報をインデックス定義に直接埋め込むこともできます：

```toml
[[tool.uv.index]]
name = "internal"
url = "https://public:koala@https://pypi-proxy.corp.dev/simple"
```

セキュリティ上の理由から、資格情報は`uv.lock`ファイルに保存されることはなく、インストール時に認証されたURLにアクセスできる必要があります。

## `--index-url`および`--extra-index-url`

`[[tool.uv.index]]`構成オプションに加えて、uvは互換性のためにpipスタイルの`--index-url`および`--extra-index-url`コマンドラインオプションをサポートしており、`--index-url`はデフォルトのインデックスを定義し、`--extra-index-url`は追加のインデックスを定義します。

これらのオプションは`[[tool.uv.index]]`構成オプションと組み合わせて使用することができ、同じ優先順位ルールに従います：

- デフォルトのインデックスは、レガシーの`--index-url`引数、推奨される`--default-index`引数、または`default = true`が設定された`[[tool.uv.index]]`エントリを介して定義されているかどうかに関係なく、常に最も低い優先度として扱われます。
- インデックスは、レガシーの`--extra-index-url`引数、推奨される`--index`引数、または`[[tool.uv.index]]`エントリを介して定義された順序で参照されます。

実際には、`--index-url`および`--extra-index-url`は名前のない`[[tool.uv.index]]`エントリと見なすことができ、前者には`default = true`が有効になっています。この文脈では、`--index-url`は`--default-index`に対応し、`--extra-index-url`は`--index`に対応します。
