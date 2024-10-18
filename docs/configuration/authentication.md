# 認証

## Git認証

uvはGitからのパッケージインストールを許可し、プライベートリポジトリへの認証に以下のスキームをサポートします。

SSHを使用する場合：

- `git+ssh://git@<hostname>/...`（例：`git+ssh://git@github.com/astral-sh/uv`）
- `git+ssh://git@<host>/...`（例：`git+ssh://git@github.com-key-2/astral-sh/uv`）

SSHの設定方法の詳細については、[GitHub SSHドキュメント](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/about-ssh)を参照してください。

パスワードまたはトークンを使用する場合：

- `git+https://<user>:<token>@<hostname>/...`（例：`git+https://git:github_pat_asdf@github.com/astral-sh/uv`）
- `git+https://<token>@<hostname>/...`（例：`git+https://github_pat_asdf@github.com/astral-sh/uv`）
- `git+https://<user>@<hostname>/...`（例：`git+https://git@github.com/astral-sh/uv`）

GitHubの個人アクセストークンを使用する場合、ユーザー名は任意です。GitHubはパスワードでの直接ログインをサポートしていませんが、他のホストではサポートされている場合があります。資格情報なしでユーザー名が提供された場合、入力を求められます。

URLに資格情報が含まれておらず、認証が必要な場合、[Git credential helper](https://git-scm.com/doc/credential-helpers)がクエリされます。

## HTTP認証

uvはパッケージレジストリのクエリ時にHTTP経由での資格情報をサポートします。

認証は以下のソースから取得され、優先順位は次の通りです：

- URL（例：`https://<user>:<password>@<hostname>/...`）
- [`netrc`](https://everything.curl.dev/usingcurl/netrc)設定ファイル
- [keyring](https://github.com/jaraco/keyring)プロバイダー（オプトインが必要）

単一のネットロケーション（スキーム、ホスト、およびポート）に対して認証が見つかった場合、それはコマンドの実行中にキャッシュされ、そのネットロケーションへの他のクエリにも使用されます。認証はuvの呼び出し間でキャッシュされません。

keyringベースの認証を有効にするには、`--keyring-provider subprocess`または`UV_KEYRING_PROVIDER=subprocess`を指定する必要があります。

認証は次のコンテキストで指定されたホストに対して使用される場合があります：

- `index-url`
- `extra-index-url`
- `find-links`
- `package @ https://...`

`pip`との違いの詳細については、[`pip`互換性ガイド](../pip/compatibility.md#registry-authentication)を参照してください。

## カスタムCA証明書

デフォルトでは、uvはバンドルされた`webpki-roots`クレートから証明書を読み込みます。`webpki-roots`はMozillaからの信頼できるルート証明書のセットであり、uvに含めることで移植性とパフォーマンスが向上します（特にmacOSでは、システムの信頼ストアを読み取る際に大幅な遅延が発生します）。

ただし、場合によっては、プラットフォームのネイティブ証明書ストアを使用したい場合があります。特に、システムの証明書ストアに含まれている企業の信頼ルート（例：必須プロキシのため）が必要な場合です。uvにシステムの信頼ストアを使用させるには、`--native-tls`コマンドラインフラグを使用するか、`UV_NATIVE_TLS`環境変数を`true`に設定します。

証明書への直接パスが必要な場合（例：CI環境）、`SSL_CERT_FILE`環境変数を証明書バンドルのパスに設定して、uvにシステムの信頼ストアの代わりにそのファイルを使用させます。

クライアント証明書認証（mTLS）が必要な場合、`SSL_CLIENT_CERT`環境変数をPEM形式のファイルのパスに設定し、そのファイルに証明書と秘密鍵の両方を含めます。

最後に、自己署名証明書を信頼するか、証明書の検証を無効にするセットアップを使用している場合、`allow-insecure-host`設定オプションを使用して特定のホストへの安全でない接続を許可するようにuvに指示できます。例えば、`pyproject.toml`に次の内容を追加すると、`example.com`への安全でない接続が許可されます：

```toml
[tool.uv]
allow-insecure-host = ["example.com"]
```

`allow-insecure-host`はホスト名（例：`localhost`）またはホスト名とポートのペア（例：`localhost:8080`）を受け取り、HTTPS接続にのみ適用されます。HTTP接続は本質的に安全でないためです。

`allow-insecure-host`は慎重に使用し、信頼できる環境でのみ使用してください。証明書の検証が行われないため、セキュリティリスクにさらされる可能性があります。

## 代替パッケージインデックスでの認証

人気のある代替Pythonパッケージインデックスでの認証の詳細については、[代替インデックス統合ガイド](../guides/integration/alternative-indexes.md)を参照してください。
