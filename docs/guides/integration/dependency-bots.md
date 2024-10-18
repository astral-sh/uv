# 依存関係ボット

依存関係を定期的に更新することは、脆弱性の回避、依存関係間の非互換性の制限、古いバージョンからの複雑なアップグレードの回避のために、ベストプラクティスとされています。さまざまなツールが、自動化されたプルリクエストを作成することで最新の状態を保つのに役立ちます。それらのいくつかはuvをサポートしているか、サポートのための作業が進行中です。

## Renovate

uvは[Renovate](https://github.com/renovatebot/renovate)によってサポートされています。

!!! note

    `uv pip compile`の出力（例：`requirements.txt`の更新）はまだサポートされていません。進捗状況は
    [renovatebot/renovate#30909](https://github.com/renovatebot/renovate/issues/30909)で追跡できます。

### `uv.lock`の出力

Renovateは`uv.lock`ファイルの存在を使用して、uvが依存関係の管理に使用されていることを判断し、
[プロジェクト依存関係](../../concepts/dependencies.md#project-dependencies)、
[オプション依存関係](../../concepts/dependencies.md#optional-dependencies)、
[開発依存関係](../../concepts/dependencies.md#development-dependencies)のアップグレードを提案します。Renovateは
`pyproject.toml`と`uv.lock`の両方のファイルを更新します。

ロックファイルは、定期的に（例えば、推移的依存関係を更新するために）リフレッシュすることもできます。
[`lockFileMaintenance`](https://docs.renovatebot.com/configuration-options/#lockfilemaintenance)オプションを有効にすることで可能です：

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  lockFileMaintenance: {
    enabled: true,
  },
}
```

### インラインスクリプトメタデータ

Renovateは、[スクリプトインラインメタデータ](../scripts.md/#declaring-script-dependencies)を使用して定義された依存関係の更新をサポートしています。

自動的にどのPythonファイルがスクリプトインラインメタデータを使用しているかを検出できないため、
[`fileMatch`](https://docs.renovatebot.com/configuration-options/#filematch)を使用してその場所を明示的に定義する必要があります。以下のように：

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  pep723: {
    fileMatch: [
      "scripts/generate_docs\\.py",
      "scripts/run_server\\.py",
    ],
  },
}
```

## Dependabot

uvのサポートはまだ利用できません。進捗状況は以下で追跡できます：

- `uv.lock`の出力については[dependabot/dependabot-core#10478](https://github.com/dependabot/dependabot-core/issues/10478)
- `uv pip compile`の出力については[dependabot/dependabot-core#10039](https://github.com/dependabot/dependabot-core/issues/10039)
