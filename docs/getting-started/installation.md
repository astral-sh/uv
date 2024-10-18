# uvのインストール

## インストール方法

uvをスタンドアロンインストーラーまたはお好みのパッケージマネージャーでインストールします。

### スタンドアロンインストーラー

uvはuvをダウンロードしてインストールするためのスタンドアロンインストーラーを提供しています：

=== "macOSとLinux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | sh
    ```

=== "Windows"

    ```console
    $ powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

特定のバージョンをリクエストするには、URLに含めます：

=== "macOSとLinux"

    ```console
    $ curl -LsSf https://astral.sh/uv/0.4.6/install.sh | sh
    ```

=== "Windows"

    ```console
    $ powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/0.4.6/install.ps1 | iex"
    ```

!!! tip

    インストールスクリプトは使用前に確認できます：

    === "macOSとLinux"

        ```console
        $ curl -LsSf https://astral.sh/uv/install.sh | less
        ```

    === "Windows"

        ```console
        $ powershell -c "irm https://astral.sh/uv/install.ps1 | more"
        ```

    また、インストーラーやバイナリは[GitHub](#github-releases)から直接ダウンロードできます。

#### インストールの設定

デフォルトでは、uvは`~/.cargo/bin`にインストールされます。インストールパスを変更するには、
`UV_INSTALL_DIR`を使用します：

=== "macOSとLinux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/custom/path" sh
    ```

=== "Windows"

    ```powershell
    $env:UV_INSTALL_DIR = "C:\Custom\Path" powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

インストーラーはまた、uvバイナリが`PATH`に含まれるようにシェルプロファイルを更新します。この動作を無効にするには、
`INSTALLER_NO_MODIFY_PATH`を使用します。例えば：

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env INSTALLER_NO_MODIFY_PATH=1 sh
```

環境変数を使用することをお勧めします。これにより、プラットフォーム間で一貫性が保たれます。ただし、オプションをインストールスクリプトに直接渡すこともできます。例えば、利用可能なオプションを確認するには：

```console
$ curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --help
```

CIのような一時的な環境では、`UV_UNMANAGED_INSTALL`を使用してuvを特定のパスにインストールし、インストーラーがシェルプロファイルや環境変数を変更しないようにします：

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env UV_UNMANAGED_INSTALL="/custom/path" sh
```

`UV_UNMANAGED_INSTALL`を使用すると、自己更新（`uv self update`経由）も無効になります。

### PyPI

便宜上、uvは[PyPI](https://pypi.org/project/uv/)に公開されています。

PyPIからインストールする場合、uvを隔離された環境にインストールすることをお勧めします。例えば、`pipx`を使用します：

```console
$ pipx install uv
```

ただし、`pip`も使用できます：

```console
$ pip install uv
```

!!! note

    uvは多くのプラットフォーム向けに事前構築されたディストリビューション（ホイール）を提供しています。特定のプラットフォーム向けのホイールが利用できない場合、uvはソースからビルドされます。これにはRustツールチェーンが必要です。ソースからuvをビルドする詳細については、
    [貢献者のセットアップガイド](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md#setup)を参照してください。

### Cargo

uvはCargo経由で利用可能ですが、未公開のクレートに依存しているため、[crates.io](https://crates.io)ではなくGitからビルドする必要があります。

```console
$ cargo install --git https://github.com/astral-sh/uv uv
```

### Homebrew

uvはcore Homebrewパッケージで利用可能です。

```console
$ brew install uv
```

### Winget

uvは[winget](https://winstall.app/apps/astral-sh.uv)経由で利用可能です。

```console
$ winget install --id=astral-sh.uv  -e
```

### Docker

uvは[`ghcr.io/astral-sh/uv`](https://github.com/astral-sh/uv/pkgs/container/uv)でDockerイメージを提供しています。

詳細については、[Dockerでuvを使用するガイド](../guides/integration/docker.md)を参照してください。

### GitHubリリース

uvのリリースアーティファクトは[GitHubリリース](https://github.com/astral-sh/uv/releases)から直接ダウンロードできます。

各リリースページには、すべてのサポートされているプラットフォーム向けのバイナリと、`github.com`経由でスタンドアロンインストーラーを使用するための手順が含まれています。

## uvのアップグレード

uvがスタンドアロンインストーラーを介してインストールされている場合、オンデマンドで自己更新できます：

```console
$ uv self update
```

!!! tip

    uvの更新はインストーラーを再実行し、シェルプロファイルを変更する可能性があります。この動作を無効にするには、`INSTALLER_NO_MODIFY_PATH=1`を設定します。

他のインストール方法が使用されている場合、自己更新は無効になります。代わりにパッケージマネージャーのアップグレード方法を使用します。例えば、`pip`を使用する場合：

```console
$ pip install --upgrade uv
```

## シェルの自動補完

uvコマンドのシェル自動補完を有効にするには、次のいずれかを実行します：

=== "LinuxとmacOS"

    ```bash
    # シェルを確認し（例：`echo $SHELL`）、次のいずれかを実行します：
    echo 'eval "$(uv generate-shell-completion bash)"' >> ~/.bashrc
    echo 'eval "$(uv generate-shell-completion zsh)"' >> ~/.zshrc
    echo 'uv generate-shell-completion fish | source' >> ~/.config/fish/config.fish
    echo 'eval (uv generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "Windows"

    ```powershell
    Add-Content -Path $PROFILE -Value '(& uv generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

uvxのシェル自動補完を有効にするには、次のいずれかを実行します：

=== "LinuxとmacOS"

    ```bash
    # シェルを確認し（例：`echo $SHELL`）、次のいずれかを実行します：
    echo 'eval "$(uvx --generate-shell-completion bash)"' >> ~/.bashrc
    echo 'eval "$(uvx --generate-shell-completion zsh)"' >> ~/.zshrc
    echo 'uvx --generate-shell-completion fish | source' >> ~/.config/fish/config.fish
    echo 'eval (uvx --generate-shell-completion elvish | slurp)'' >> ~/.elvish/rc.elv
    ```

=== "Windows"

    ```powershell
    Add-Content -Path $PROFILE -Value '(& uvx --generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

その後、シェルを再起動するか、シェルの設定ファイルをソースにします。

## アンインストール

システムからuvを削除する必要がある場合は、`uv`および`uvx`バイナリを削除します：

=== "macOSとLinux"

    ```console
    $ rm ~/.cargo/bin/uv ~/.cargo/bin/uvx
    ```

=== "Windows"

    ```powershell
    $ rm $HOME\.cargo\bin\uv.exe
    $ rm $HOME\.cargo\bin\uvx.exe
    ```

!!! tip

    バイナリを削除する前に、uvが保存したデータを削除することをお勧めします：

    ```console
    $ uv cache clean
    $ rm -r "$(uv python dir)"
    $ rm -r "$(uv tool dir)"
    ```

## 次のステップ

[最初のステップ](./first-steps.md)を確認するか、[ガイド](../guides/index.md)に直接ジャンプしてuvの使用を開始してください。
