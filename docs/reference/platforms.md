# プラットフォームサポート

uvは以下のプラットフォームに対してTier 1のサポートを提供しています：

- macOS (Apple Silicon)
- macOS (x86_64)
- Linux (x86_64)
- Windows (x86_64)

uvはTier 1プラットフォームに対して継続的にビルド、テスト、および開発されています。Rustプロジェクトに触発され、Tier 1は
["動作保証"](https://doc.rust-lang.org/beta/rustc/platform-support.html)と考えることができます。

uvは以下のプラットフォームに対してTier 2のサポート
(["ビルド保証"](https://doc.rust-lang.org/beta/rustc/platform-support.html))を提供しています：

- Linux (PPC64)
- Linux (PPC64LE)
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)

uvはTier 1およびTier 2プラットフォーム向けに[PyPI](https://pypi.org/project/uv/)に事前ビルドされたホイールを提供します。ただし、Tier 2プラットフォームは継続的にビルドされますが、継続的にテストまたは開発されていないため、実際の安定性は異なる場合があります。

Tier 1およびTier 2プラットフォーム以外では、uvはi686 Windowsでビルドされることが知られており、aarch64 Windowsではビルドされないことが知られていますが、現時点ではどちらのプラットフォームもサポートされているとは見なされていません。サポートされている最小のWindowsバージョンは、Windows 10およびWindows Server 2016であり、これは
[RustのTier 1サポート](https://blog.rust-lang.org/2024/02/26/Windows-7.html)に従っています。

uvはPython 3.8、3.9、3.10、3.11、3.12、および3.13に対してサポートおよびテストされています。
