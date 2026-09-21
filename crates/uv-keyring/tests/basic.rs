#![cfg(feature = "native-auth")]

use common::{generate_random_bytes_of_len, generate_random_string, init_logger};
use uv_keyring::{Entry, Error};

mod common;

#[tokio::test]
async fn test_missing_entry() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Missing entry has password"
    );
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn test_empty_password() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let in_pass = "";
    entry
        .set_password(in_pass)
        .await
        .expect("Can't set empty password");
    let out_pass = entry
        .get_password()
        .await
        .expect("Can't get empty password");
    assert_eq!(
        in_pass, out_pass,
        "Retrieved and set empty passwords don't match"
    );
    entry
        .delete_credential()
        .await
        .expect("Can't delete password");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted password"
    );
}

#[tokio::test]
async fn test_round_trip_ascii_password() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let password = "test ascii password";
    entry
        .set_password(password)
        .await
        .expect("Can't set ascii password");
    let stored_password = entry
        .get_password()
        .await
        .expect("Can't get ascii password");
    assert_eq!(
        stored_password, password,
        "Retrieved and set ascii passwords don't match"
    );
    entry
        .delete_credential()
        .await
        .expect("Can't delete ascii password");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted ascii password"
    );
}

#[tokio::test]
async fn test_round_trip_non_ascii_password() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let password = "このきれいな花は桜です";
    entry
        .set_password(password)
        .await
        .expect("Can't set non-ascii password");
    let stored_password = entry
        .get_password()
        .await
        .expect("Can't get non-ascii password");
    assert_eq!(
        stored_password, password,
        "Retrieved and set non-ascii passwords don't match"
    );
    entry
        .delete_credential()
        .await
        .expect("Can't delete non-ascii password");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted non-ascii password"
    );
}

#[tokio::test]
async fn test_round_trip_random_secret() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let secret = generate_random_bytes_of_len(24);
    entry
        .set_secret(secret.as_slice())
        .await
        .expect("Can't set random secret");
    let stored_secret = entry.get_secret().await.expect("Can't get random secret");
    assert_eq!(
        &stored_secret,
        secret.as_slice(),
        "Retrieved and set random secrets don't match"
    );
    entry
        .delete_credential()
        .await
        .expect("Can't delete random secret");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted random secret"
    );
}

#[tokio::test]
async fn test_update() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let password = "test ascii password";
    entry
        .set_password(password)
        .await
        .expect("Can't set initial ascii password");
    let stored_password = entry
        .get_password()
        .await
        .expect("Can't get ascii password");
    assert_eq!(
        stored_password, password,
        "Retrieved and set initial ascii passwords don't match"
    );
    let password = "このきれいな花は桜です";
    entry
        .set_password(password)
        .await
        .expect("Can't update ascii with non-ascii password");
    let stored_password = entry
        .get_password()
        .await
        .expect("Can't get non-ascii password");
    assert_eq!(
        stored_password, password,
        "Retrieved and updated non-ascii passwords don't match"
    );
    entry
        .delete_credential()
        .await
        .expect("Can't delete updated password");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted updated password"
    );
}
