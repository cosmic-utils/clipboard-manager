use std::{
    fs,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use serial_test::serial;

use anyhow::Result;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::Config,
    db::{DbSqlite, DbTrait},
};

use super::MimeDataMap;

fn prepare_db_dir() -> PathBuf {
    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "warn,{}=info",
        env!("CARGO_CRATE_NAME")
    )));
    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init();

    let db_dir = PathBuf::from("tests");
    let _ = std::fs::create_dir_all(&db_dir);
    remove_dir_contents(&db_dir);
    db_dir
}

#[tokio::test]
#[serial]
async fn test() -> Result<()> {
    let db_dir = prepare_db_dir();

    let mut db = DbSqlite::with_path(&Config::default(), &db_dir).await?;

    test_db(&mut db).await.unwrap();

    db.clear().await?;

    test_db(&mut db).await.unwrap();

    Ok(())
}

fn build_content(content: &[(&str, &str)]) -> MimeDataMap {
    content
        .iter()
        .map(|(mime, content)| (mime.to_string(), content.as_bytes().into()))
        .collect()
}

async fn test_db(db: &mut DbSqlite) -> Result<()> {
    assert!(db.len() == 0);

    let data = build_content(&[("text/plain", "content")]);

    db.insert_with_time(data.clone(), 10).await.unwrap();

    assert!(db.len() == 1);

    sleep(Duration::from_millis(1000));

    db.insert_with_time(data.clone(), 20).await.unwrap();

    assert!(db.len() == 1);

    sleep(Duration::from_millis(1000));

    let data2 = build_content(&[("text/plain", "content2")]);

    db.insert_with_time(data2.clone(), 30).await.unwrap();

    assert_eq!(db.len(), 2);

    let next = db.iter().next().unwrap();

    assert!(next.raw_content == data2);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_delete_old_one() {
    let db_path = prepare_db_dir();

    let mut db = DbSqlite::with_path(&Config::default(), &db_path)
        .await
        .unwrap();

    let data = build_content(&[("text/plain", "content")]);

    db.insert(data).await.unwrap();

    sleep(Duration::from_millis(100));

    let data = build_content(&[("text/plain", "content2")]);

    db.insert(data).await.unwrap();

    assert_eq!(db.len(), 2);

    let db = DbSqlite::with_path(&Config::default(), &db_path)
        .await
        .unwrap();

    assert_eq!(db.len(), 2);

    let config = Config {
        maximum_entries_lifetime: Some(0),
        ..Default::default()
    };
    let db = DbSqlite::with_path(&config, &db_path).await.unwrap();

    assert_eq!(db.len(), 0);
}

#[tokio::test]
#[serial]
async fn same() {
    let db_path = prepare_db_dir();

    let mut db = DbSqlite::with_path(&Config::default(), &db_path)
        .await
        .unwrap();

    let data = build_content(&[("text/plain", "content")]);

    db.insert(data.clone()).await.unwrap();
    db.insert(data.clone()).await.unwrap();
    assert!(db.len() == 1);
}

#[tokio::test]
#[serial]
async fn favorites() {
    let db_path = prepare_db_dir();

    let mut db = DbSqlite::with_path(&Config::default(), &db_path)
        .await
        .unwrap();

    let now1 = 1000;
    let data1 = build_content(&[("text/plain", "content1")]);
    db.insert_with_time(data1, now1).await.unwrap();

    let now2 = 2000;
    let data2 = build_content(&[("text/plain", "content2")]);
    db.insert_with_time(data2, now2).await.unwrap();

    let now3 = 3000;
    let data3 = build_content(&[("text/plain", "content3")]);
    db.insert_with_time(data3.clone(), now3).await.unwrap();

    db.add_favorite(now3, None).await.unwrap();

    assert!(db.get_from_id(now3).unwrap().is_favorite);
    assert_eq!(db.favorites.len(), 1);

    db.delete(now3).await.unwrap();

    assert_eq!(db.favorites.len(), 0);

    db.insert_with_time(data3.clone(), now3).await.unwrap();

    db.add_favorite(now1, None).await.unwrap();

    db.add_favorite(now3, None).await.unwrap();

    db.add_favorite(now2, Some(1)).await.unwrap();

    assert_eq!(db.favorites.len(), 3);

    assert_eq!(db.favorites.fav(), &vec![now1, now2, now3]);

    db.remove_favorite(now2).await.unwrap();

    assert_eq!(db.len(), 3);

    let db = DbSqlite::with_path(
        &Config {
            maximum_entries_lifetime: Some(0),
            ..Default::default()
        },
        &db_path,
    )
    .await
    .unwrap();

    assert_eq!(db.len(), 2);

    assert_eq!(db.favorites.len(), 2);
    assert_eq!(db.favorites.fav(), &vec![now1, now3]);
}

fn remove_dir_contents(dir: &Path) {
    pub fn inner(dir: &Path) -> Result<(), std::io::Error> {
        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();

            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
        Ok(())
    }

    let _ = inner(dir);
}

use std::time::Instant;

#[tokio::test]
#[ignore = "bench"]
async fn bench_search_from_system_path() {
    let mut db = DbSqlite::new(&Config::default()).await.unwrap();

    let now = Instant::now();

    println!("{}", db.len());

    db.set_query_and_search("a".into());

    println!("Elapsed: {:?}", now.elapsed());
}
