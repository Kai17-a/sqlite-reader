# sqlite-reader

`ratatui` で作られた、SQLiteデータベースを読み取り専用で閲覧するTUIアプリです。テーブルの切り替え、行の選択、選択行の詳細表示に対応しています。

## 使い方

テスト用データベースを作成します。

```bash
cargo run --bin create_sample_db
```

SQLiteデータベースを開きます。

```bash
cargo run -- sample.sqlite
```
