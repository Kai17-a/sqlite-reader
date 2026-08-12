# sqlite-reader

`ratatui` で作られた、SQLiteデータベースを読み取り専用で閲覧するTUIアプリです。テーブルの切り替え、行の選択、選択行の詳細表示に対応しています。

## インストール

GitHub Releasesから`mise`でインストールします。

```bash
mise use -g github:Kai17-a/sqlite-reader
```

特定バージョンを使う場合:

```bash
mise use -g github:Kai17-a/sqlite-reader@0.1.0
```

## コマンド

SQLiteデータベースを開きます。

```bash
sqlite-reader path/to/database.sqlite
```

コマンドのヘルプとバージョンを表示します。

```bash
sqlite-reader --help
sqlite-reader --version
```

TUI内では`←/→`（または`h/l`）でテーブルを切り替え、`↑/↓`（または`j/k`）で行を選択します。`r`で再読み込み、`q`または`Esc`で終了します。

## 開発用: サンプルデータベース

テスト用データベースを作成します。

```bash
cargo run --bin create_sample_db
```

SQLiteデータベースを開きます。

```bash
cargo run -- sample.sqlite
```

サンプルDB作成コマンドのヘルプ:

```bash
cargo run --bin create_sample_db -- --help
```
