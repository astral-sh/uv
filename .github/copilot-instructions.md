## Translation instructions into Japanese

Below are instructions to the Copilot Workspace in Japanese.

Copilot Workspace への指示が「ドキュメントの日本語への翻訳」である場合は、以下のルールに従ってください。


### 前提条件

「ドキュメントの翻訳」とは Markdown ファイル (.md) または reStructuedText ファイル (.rst) の文章を翻訳することを指します。 殆どの場合ドキュメントは英語となっており、Copilot Workspace に指示される翻訳対象は日本語です。

### ルール

1. 文章の部分と、コードブロック内のコメント部分のみを翻訳します。 Markdown や reStructuredText の構文は翻訳しません。
    - 例:
        - 原文:
            ```
            > ![note]
            > Python is a programming language that lets you work quickly and integrate systems more effectively.
            ```
        - NG:
            ```
            > ![ノート]
            > Pythonは、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
        - OK:
            ```
            > ![note]
            > Python は、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
1.  「ヘッダー」は翻訳しません。 原文のままにします。
    - 例 (Markdown):
        - 原文:
            ```
            # Python is a programming language
            ## Python is a programming language
            ### Python is a programming language
            ```
        - NG:
            ```
            # Python is a programming language
            ## Python is a programming language
            ### Python is a programming language
            ```
        - OK:
            ```
            ## Python is a programming language
            ```
    - 例 (reStructuredText):
        - 原文:
            ```
            Python is a programming language
            --------------------------------
            ```
        - NG:
            ```
            Python はプログラミング言語です
            --------------------------------
            ```
        - OK:
            ```
            Python is a programming language
            --------------------------------
            ```
    - 例 (reStructuredText with overline):
        - 原文:
            ```
            --------------------------------
            Python is a programming language
            --------------------------------
            ```
        - NG:
            ```
            --------------------------------
            Python はプログラミング言語です
            --------------------------------
            ```
        - OK:
            ```
            --------------------------------
            Python is a programming language
            --------------------------------
            ```
1. 英語や半角記号と、日本語の単語の間には半角スペースを挿入します。
    - 例:
        - 原文:
            ```
            Python is a programming language that lets you work quickly and integrate systems more effectively.
            ```
        - NG:
            ```
            Pythonは、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
        - OK:
            ```
            Python は、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
1. インラインコードブロックと日本語の単語の間には半角スペースを挿入します。
    - 例:
        - 原文:
            ```
            `Python` is a programming language that lets you work quickly and integrate systems more effectively.
            ```
        - NG:
            ```
            `Python`は、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
        - OK:
            ```
            `Python` は、作業を迅速に進め、システムを効率的に統合できるプログラミング言語です。
            ```
1. 句読点以外の記号は半角のままにします。 全角にしません。
    - 例:
        - 原文:
            ```
            Python is a programming language that lets you work quickly (and integrate systems more effectively):
            ```
        - NG:
            ```
            Python は、作業を迅速に進め（、システムを効率的に統合できる）プログラミング言語です：
            ```
        - OK:
            ```
            Python は、作業を迅速に進め(、システムを効率的に統合できる) プログラミング言語です:
            ```
