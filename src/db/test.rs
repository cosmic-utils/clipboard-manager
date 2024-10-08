use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    thread::sleep,
    time::Duration,
};

use serial_test::serial;

use anyhow::Result;
use cosmic::{iced_sctk::util, widget::canvas::Path};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::Config,
    utils::{self, remove_dir_contents},
};

use crate::db::{Db, Entry};

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

    let mut db = Db::inner_new(&Config::default(), &db_dir).await?;

    test_db(&mut db).await.unwrap();

    db.clear().await?;

    test_db(&mut db).await.unwrap();

    Ok(())
}

async fn test_db(db: &mut Db) -> Result<()> {
    assert!(db.len() == 0);

    let data = Entry::new_now(
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();

    assert!(db.len() == 1);

    sleep(Duration::from_millis(1000));

    let data = Entry::new_now(
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();

    assert!(db.len() == 1);

    sleep(Duration::from_millis(1000));

    let data = Entry::new_now(
        "text/plain".into(),
        "content2".as_bytes().into(),
        None,
        false,
    );

    db.insert(data.clone()).await.unwrap();

    assert!(db.len() == 2);

    let next = db.iter().next().unwrap();

    assert!(next.creation == data.creation);
    assert!(next.content == data.content);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_delete_old_one() {
    let db_path = prepare_db_dir();

    let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

    let data = Entry::new_now(
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );
    db.insert(data).await.unwrap();

    sleep(Duration::from_millis(100));

    let data = Entry::new_now(
        "text/plain".into(),
        "content2".as_bytes().into(),
        None,
        false,
    );
    db.insert(data).await.unwrap();

    assert!(db.len() == 2);

    let db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

    assert!(db.len() == 2);

    let config = Config {
        maximum_entries_lifetime: Some(0),
        ..Default::default()
    };
    let db = Db::inner_new(&config, &db_path).await.unwrap();

    assert!(db.len() == 0);
}

#[tokio::test]
#[serial]
async fn same() {
    let db_path = prepare_db_dir();

    let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

    let now = utils::now_millis();

    let data = Entry::new(
        now,
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();

    let data = Entry::new(
        now,
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();
    assert!(db.len() == 1);
}

#[tokio::test]
#[serial]
async fn different_content_same_time() {
    let db_path = prepare_db_dir();

    let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

    let now = utils::now_millis();

    let data = Entry::new(
        now,
        "text/plain".into(),
        "content".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();

    let data = Entry::new(
        now,
        "text/plain".into(),
        "content2".as_bytes().into(),
        None,
        false,
    );

    db.insert(data).await.unwrap();
    assert!(db.len() == 2);
}

#[tokio::test]
#[serial]
async fn favorites() {
    let db_path = prepare_db_dir();

    let mut db = Db::inner_new(&Config::default(), &db_path).await.unwrap();

    let now1 = 1000;

    let data1 = Entry::new(
        now1,
        "text/plain".into(),
        "content1".as_bytes().into(),
        None,
        false,
    );

    db.insert(data1).await.unwrap();

    let now2 = 2000;

    let data2 = Entry::new(
        now2,
        "text/plain".into(),
        "content2".as_bytes().into(),
        None,
        false,
    );

    db.insert(data2).await.unwrap();

    let now3 = 3000;

    let data3 = Entry::new(
        now3,
        "text/plain".into(),
        "content3".as_bytes().into(),
        None,
        false,
    );

    db.insert(data3.clone()).await.unwrap();

    db.add_favorite(&db.state.get(&now3).unwrap().clone(), None)
        .await
        .unwrap();

    db.delete(&db.state.get(&now3).unwrap().clone())
        .await
        .unwrap();

    assert_eq!(db.favorite_len(), 0);

    db.insert(data3).await.unwrap();

    db.add_favorite(&db.state.get(&now1).unwrap().clone(), None)
        .await
        .unwrap();

    db.add_favorite(&db.state.get(&now3).unwrap().clone(), None)
        .await
        .unwrap();

    db.add_favorite(&db.state.get(&now2).unwrap().clone(), Some(1))
        .await
        .unwrap();

    assert_eq!(db.favorite_len(), 3);

    assert_eq!(db.favorites.fav(), &vec![now1, now2, now3]);

    let db = Db::inner_new(
        &Config {
            maximum_entries_lifetime: None,
            ..Default::default()
        },
        &db_path,
    )
    .await
    .unwrap();

    assert_eq!(db.len(), 3);

    assert_eq!(db.favorite_len(), 3);
    assert_eq!(db.favorites.fav(), &vec![now1, now2, now3]);
}
