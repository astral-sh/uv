# 環境変数

uvは次のコマンドライン引数を環境変数として受け入れます：

- `UV_INDEX`: `--index`コマンドライン引数と同等です。設定されている場合、uvはパッケージを検索する際にこのスペースで区切られたURLのリストを追加のインデックスとして使用します。
- `UV_DEFAULT_INDEX`: `--default-index`コマンドライン引数と同等です。設定されている場合、uvはパッケージを検索する際にこのURLをデフォルトのインデックスとして使用します。
- `UV_INDEX_URL`: `--index-url`コマンドライン引数と同等です。設定されている場合、uvはパッケージを検索する際にこのURLをデフォルトのインデックスとして使用します。（非推奨：代わりに`UV_DEFAULT_INDEX`を使用してください。）
- `UV_EXTRA_INDEX_URL`: `--extra-index-url`コマンドライン引数と同等です。設定されている場合、uvはパッケージを検索する際にこのスペースで区切られたURLのリストを追加のインデックスとして使用します。（非推奨：代わりに`UV_INDEX`を使用してください。）
- `UV_FIND_LINKS`: `--find-links`コマンドライン引数と同等です。設定されている場合、uvはパッケージを検索するための追加の場所としてこのカンマで区切られたリストを使用します。
- `UV_CACHE_DIR`: `--cache-dir`コマンドライン引数と同等です。設定されている場合、uvはデフォルトのキャッシュディレクトリの代わりにこのディレクトリをキャッシュに使用します。
- `UV_NO_CACHE`: `--no-cache`コマンドライン引数と同等です。設定されている場合、uvはすべての操作でキャッシュを使用しません。
- `UV_RESOLUTION`: `--resolution`コマンドライン引数と同等です。例えば、`lowest-direct`に設定されている場合、uvはすべての直接依存関係の最も互換性のあるバージョンをインストールします。
- `UV_PRERELEASE`: `--prerelease`コマンドライン引数と同等です。例えば、`allow`に設定されている場合、uvはすべての依存関係のプレリリースバージョンを許可します。
- `UV_SYSTEM_PYTHON`: `--system`コマンドライン引数と同等です。`true`に設定されている場合、uvはシステムの`PATH`で最初に見つかったPythonインタープリタを使用します。警告：`UV_SYSTEM_PYTHON=true`は継続的インテグレーション（CI）またはコンテナ化された環境での使用を意図しており、システムPythonを変更すると予期しない動作が発生する可能性があるため、注意して使用してください。
- `UV_PYTHON`: `--python`コマンドライン引数と同等です。パスに設定されている場合、uvはすべての操作でこのPythonインタープリタを使用します。
- `UV_BREAK_SYSTEM_PACKAGES`: `--break-system-packages`コマンドライン引数と同等です。`true`に設定されている場合、uvはシステムにインストールされたパッケージと競合するパッケージのインストールを許可します。警告：`UV_BREAK_SYSTEM_PACKAGES=true`は継続的インテグレーション（CI）またはコンテナ化された環境での使用を意図しており、システムPythonを変更すると予期しない動作が発生する可能性があるため、注意して使用してください。
- `UV_NATIVE_TLS`: `--native-tls`コマンドライン引数と同等です。`true`に設定されている場合、uvはバンドルされた`webpki-roots`クレートの代わりにシステムの信頼ストアを使用します。
- `UV_INDEX_STRATEGY`: `--index-strategy`コマンドライン引数と同等です。例えば、`unsafe-any-match`に設定されている場合、uvはすべてのインデックスURLで利用可能な特定のパッケージのバージョンを考慮し、最初のインデックスURLに限定せずに検索します。
- `UV_REQUIRE_HASHES`: `--require-hashes`コマンドライン引数と同等です。`true`に設定されている場合、uvはすべての依存関係に要件ファイルにハッシュが指定されていることを要求します。
- `UV_CONSTRAINT`: `--constraint`コマンドライン引数と同等です。設定されている場合、uvはこのファイルを制約ファイルとして使用します。スペースで区切られたファイルのリストを使用します。
- `UV_BUILD_CONSTRAINT`: `--build-constraint`コマンドライン引数と同等です。設定されている場合、uvはソースディストリビューションのビルドに対する制約としてこのファイルを使用します。スペースで区切られたファイルのリストを使用します。
- `UV_OVERRIDE`: `--override`コマンドライン引数と同等です。設定されている場合、uvはこのファイルをオーバーライドファイルとして使用します。スペースで区切られたファイルのリストを使用します。
- `UV_LINK_MODE`: `--link-mode`コマンドライン引数と同等です。設定されている場合、uvはこれをリンクモードとして使用します。
- `UV_NO_BUILD_ISOLATION`: `--no-build-isolation`コマンドライン引数と同等です。設定されている場合、uvはソースディストリビューションのビルド時に分離をスキップします。
- `UV_CUSTOM_COMPILE_COMMAND`: `--custom-compile-command`コマンドライン引数と同等です。`uv pip compile`によって生成された`requirements.txt`ファイルの出力ヘッダーでuvをオーバーライドするために使用されます。ラッパースクリプト内から`uv pip compile`が呼び出されるユースケースを意図しており、出力ファイルにラッパースクリプトの名前を含めます。
- `UV_KEYRING_PROVIDER`: `--keyring-provider`コマンドライン引数と同等です。設定されている場合、uvはこの値をキーチェーンプロバイダーとして使用します。
- `UV_CONFIG_FILE`: `--config-file`コマンドライン引数と同等です。ローカルの`uv.toml`ファイルへのパスを期待します。
- `UV_NO_CONFIG`: `--no-config`コマンドライン引数と同等です。設定されている場合、uvは現在のディレクトリ、親ディレクトリ、またはユーザー設定ディレクトリから設定ファイルを読み取りません。
- `UV_EXCLUDE_NEWER`: `--exclude-newer`コマンドライン引数と同等です。設定されている場合、uvは指定された日付以降に公開されたディストリビューションを除外します。
- `UV_PYTHON_PREFERENCE`: `--python-preference`コマンドライン引数と同等です。uvがシステムまたは管理されたPythonバージョンを優先するかどうかを指定します。
- `UV_PYTHON_DOWNLOADS`: [`python-downloads`](../reference/settings.md#python-downloads)設定および無効化された場合の`--no-python-downloads`オプションと同等です。uvがPythonのダウンロードを許可するかどうかを指定します。
- `UV_COMPILE_BYTECODE`: `--compile-bytecode`コマンドライン引数と同等です。設定されている場合、uvはインストール後にPythonソースファイルをバイトコードにコンパイルします。
- `UV_PUBLISH_URL`: `--publish-url`コマンドライン引数と同等です。`uv publish`で使用するインデックスのアップロードエンドポイントのURLです。
- `UV_PUBLISH_TOKEN`: `uv publish`の`--token`コマンドライン引数と同等です。設定されている場合、uvはこのトークンを（ユーザー名`__token__`と共に）公開に使用します。
- `UV_PUBLISH_USERNAME`: `uv publish`の`--username`コマンドライン引数と同等です。設定されている場合、uvは公開にこのユーザー名を使用します。
- `UV_PUBLISH_PASSWORD`: `uv publish`の`--password`コマンドライン引数と同等です。設定されている場合、uvは公開にこのパスワードを使用します。
- `UV_NO_SYNC`: `--no-sync`コマンドライン引数と同等です。設定されている場合、uvは環境の更新をスキップします。

いずれの場合も、対応するコマンドライン引数が環境変数よりも優先されます。

さらに、uvは次の環境変数を尊重します：

- `UV_CONCURRENT_DOWNLOADS`: uvが任意の時点で実行する最大同時ダウンロード数を設定します。
- `UV_CONCURRENT_BUILDS`: uvが任意の時点で同時にビルドするソースディストリビューションの最大数を設定します。
- `UV_CONCURRENT_INSTALLS`: パッケージのインストールおよび解凍時に使用するスレッド数を制御するために使用されます。
- `UV_TOOL_DIR`: uvが管理ツールを保存するディレクトリを指定するために使用されます。
- `UV_TOOL_BIN_DIR`: uvがツールの実行可能ファイルをインストールする「bin」ディレクトリを指定するために使用されます。
- `UV_PROJECT_ENVIRONMENT`: プロジェクト仮想環境に使用するディレクトリのパスを指定するために使用されます。詳細については、[プロジェクトドキュメント](../concepts/projects.md#configuring-the-project-environment-path)を参照してください。
- `UV_PYTHON_INSTALL_DIR`: uvが管理されたPythonインストールを保存するディレクトリを指定するために使用されます。
- `UV_PYTHON_INSTALL_MIRROR`: 管理されたPythonインストールは[`python-build-standalone`](https://github.com/indygreg/python-build-standalone)からダウンロードされます。この変数をミラーURLに設定して、Pythonインストールの別のソースを使用できます。提供されたURLは、例えば`https://github.com/indygreg/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`のように、`https://github.com/indygreg/python-build-standalone/releases/download`を置き換えます。ローカルディレクトリからのディストリビューションは、`file://` URLスキームを使用して読み取ることができます。
- `UV_PYPY_INSTALL_MIRROR`: 管理されたPyPyインストールは[python.org](https://downloads.python.org/)からダウンロードされます。この変数をミラーURLに設定して、PyPyインストールの別のソースを使用できます。提供されたURLは、例えば`https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`のように、`https://downloads.python.org/pypy`を置き換えます。ローカルディレクトリからのディストリビューションは、`file://` URLスキームを使用して読み取ることができます。
- `XDG_CONFIG_HOME`: Unixシステム上のuvユーザーレベル設定ディレクトリのパスを指定するために使用されます。
- `XDG_CACHE_HOME`: Unixシステム上でuvがキャッシュファイルを保存するディレクトリを指定するために使用されます。
- `XDG_DATA_HOME`: Unixシステム上でuvが管理されたPythonインストールおよび管理ツールを保存するディレクトリを指定するために使用されます。
- `XDG_BIN_HOME`: 実行可能ファイルがインストールされるディレクトリを指定するために使用されます。
- `SSL_CERT_FILE`: 設定されている場合、uvはシステムの信頼ストアの代わりにこのファイルを証明書バンドルとして使用します。
- `SSL_CLIENT_CERT`: 設定されている場合、uvはこのファイルをmTLS認証に使用します。これは、証明書と秘密鍵の両方をPEM形式で含む単一のファイルである必要があります。
- `RUST_LOG`: 設定されている場合、uvは`--verbose`出力のログレベルとしてこの値を使用します。`tracing_subscriber`クレートと互換性のあるフィルタを受け入れます。例えば、`RUST_LOG=trace`はトレースレベルのログを有効にします。詳細については、[tracingドキュメント](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)を参照してください。
- `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`: すべてのHTTP/HTTPSリクエストに使用するプロキシ。
- `HTTP_TIMEOUT`（または`UV_HTTP_TIMEOUT`）: 設定されている場合、uvはHTTP読み取りのタイムアウトとしてこの値（秒単位）を使用します（デフォルト：30秒）。
- `PYC_INVALIDATION_MODE`: `--compile`で実行する際に使用する検証モード。詳細については、[`PycInvalidationMode`](https://docs.python.org/3/library/py_compile.html#py_compile.PycInvalidationMode)を参照してください。
- `VIRTUAL_ENV`: アクティブ化された仮想環境を検出するために使用されます。
- `CONDA_PREFIX`: アクティブ化されたConda環境を検出するために使用されます。
- `PROMPT`: Windowsコマンドプロンプト（PowerShellではなく）の使用を検出するために使用されます。
- `VIRTUAL_ENV_DISABLE_PROMPT`: 仮想環境がアクティブ化される前に`1`に設定されている場合、仮想環境名はターミナルプロンプトに追加されません。
- `NU_VERSION`: NuShellの使用を検出するために使用されます。
- `FISH_VERSION`: Fishシェルの使用を検出するために使用されます。
- `BASH_VERSION`: Bashシェルの使用を検出するために使用されます。
- `ZSH_VERSION`: Zshシェルの使用を検出するために使用されます。
- `MACOSX_DEPLOYMENT_TARGET`: `--python-platform macos`および関連するバリアントで使用され、デプロイメントターゲット（つまり、サポートされる最小のmacOSバージョン）を設定します。デフォルトは`12.0`で、執筆時点での最も古い非EOLのmacOSバージョンです。
- `NO_COLOR`: 色を無効にします。`FORCE_COLOR`よりも優先されます。詳細については、[no-color.org](https://no-color.org)を参照してください。
- `FORCE_COLOR`: TTYサポートに関係なく色を強制します。詳細については、[force-color.org](https://force-color.org)を参照してください。
