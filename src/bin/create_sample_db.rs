use std::{env, error::Error, path::Path};

use rusqlite::{Connection, params};

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "sample.sqlite".to_owned());

    if Path::new(&path).exists() {
        eprintln!("Refusing to overwrite existing file: {path}");
        eprintln!("Choose another path or remove the file first.");
        std::process::exit(2);
    }

    if let Err(error) = create_sample_database(&path) {
        eprintln!("create-sample-db: {error}");
        std::process::exit(1);
    }
    println!("Created sample SQLite database: {path}");
}

fn create_sample_database(path: &str) -> Result<(), Box<dyn Error>> {
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE users (
            id          INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            email       TEXT NOT NULL UNIQUE,
            active      INTEGER NOT NULL,
            created_at  TEXT NOT NULL
        );

        CREATE TABLE products (
            id          INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            price_yen   INTEGER NOT NULL,
            image       BLOB
        );

        CREATE TABLE orders (
            id          INTEGER PRIMARY KEY,
            user_id     INTEGER NOT NULL REFERENCES users(id),
            product_id  INTEGER NOT NULL REFERENCES products(id),
            quantity    INTEGER NOT NULL,
            note        TEXT,
            ordered_at  TEXT NOT NULL
        );
        ",
    )?;

    let mut insert_user = connection
        .prepare("INSERT INTO users (name, email, active, created_at) VALUES (?1, ?2, ?3, ?4)")?;
    for (name, email, active, created_at) in [
        (
            "佐藤 花子",
            "hanako@example.test",
            1,
            "2026-08-01T09:15:00Z",
        ),
        ("鈴木 太郎", "taro@example.test", 1, "2026-08-04T13:30:00Z"),
        ("Alex Kim", "alex@example.test", 0, "2026-08-10T18:00:00Z"),
    ] {
        insert_user.execute(params![name, email, active, created_at])?;
    }

    let mut insert_product =
        connection.prepare("INSERT INTO products (name, price_yen, image) VALUES (?1, ?2, ?3)")?;
    insert_product.execute(params!["SQLite 入門", 2800, Option::<Vec<u8>>::None])?;
    insert_product.execute(params![
        "TUI ステッカー",
        500,
        vec![0x89_u8, 0x50, 0x4e, 0x47]
    ])?;
    insert_product.execute(params!["USB キーボード", 7600, Option::<Vec<u8>>::None])?;

    let mut insert_order = connection.prepare(
        "INSERT INTO orders (user_id, product_id, quantity, note, ordered_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (user_id, product_id, quantity, note, ordered_at) in [
        (1, 1, 1, Some("ギフト包装希望"), "2026-08-05T10:20:00Z"),
        (1, 2, 3, None, "2026-08-05T10:21:00Z"),
        (2, 3, 1, Some("平日配送"), "2026-08-11T03:45:00Z"),
    ] {
        insert_order.execute(params![user_id, product_id, quantity, note, ordered_at])?;
    }

    Ok(())
}
