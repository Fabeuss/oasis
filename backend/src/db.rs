use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
use sqlx::{ConnectOptions, Connection, FromRow};
use std::path::PathBuf;
use tokio::fs;
use crate::utils;

pub async fn get_db_conn() -> SqlitePool {
    let dir = utils::get_config_dir();
    let db_file = dir.join("main.db");
    if !db_file.as_path().exists() {
        if let Err(e) = create_db(&db_file).await {
            panic!("Cannot create database\n {}", e);
        }
    }

    match get_conn_pool(&db_file).await {
        Ok(pool) => pool,
        Err(e) => panic!("Cannot get database connection pool\n {}", e),
    }
}

async fn create_db(db_file: &PathBuf) -> anyhow::Result<()> {
    let prefix = db_file
        .parent()
        .expect("Cannot retrieve the parent dir of db file");
    fs::create_dir_all(&prefix).await?;

    let mut conn = SqliteConnectOptions::new()
        .filename(db_file)
        .create_if_missing(true)
        //.log_statements(log::LevelFilter::Debug)
        .connect()
        .await?;

    let sql_init_file = std::env::var("INIT_SQLITE_FILE")
        .expect("Cannot get init SQL file from env");
    let sql = fs::read_to_string(sql_init_file).await?;
    sqlx::query(&sql).execute(&mut conn).await?;
    debug!("Database created at {:?}", db_file);
    conn.close();

    Ok(())
}

async fn get_conn_pool(db_file: &PathBuf) -> anyhow::Result<SqlitePool> {
    let db_file_str = db_file.to_str().expect("Cannot parse database filename");
    let option = SqliteConnectOptions::new().filename(db_file_str);
    //option.log_statements(log::LevelFilter::Trace);
    let pool = SqlitePool::connect_with(option).await?;

    Ok(pool)
}

pub async fn fetch_single<T>(sql: &str, args: Vec<String>, pool: &SqlitePool) -> anyhow::Result<T>
where
    T: Send + Unpin + for<'a> FromRow<'a, SqliteRow>,
{
    let stmt = prepare_sql(sql, args);
    Ok(stmt.fetch_one(pool).await?)
}

pub async fn fetch_multiple<T>(
    sql: &str,
    args: Vec<String>,
    pool: &SqlitePool,
) -> anyhow::Result<Vec<T>>
where
    T: Send + Unpin + for<'a> FromRow<'a, SqliteRow>,
{
    let stmt = prepare_sql(sql, args);
    Ok(stmt.fetch_all(pool).await?)
}

fn prepare_sql<T>(
    sql: &str,
    args: Vec<String>,
) -> sqlx::query::QueryAs<'_, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'_>>
where
    T: Send + Unpin + for<'a> FromRow<'a, SqliteRow>,
{
    let mut stmt = sqlx::query_as(sql);
    for arg in args.iter() {
        stmt = stmt.bind(arg.to_owned());
    }

    stmt
}