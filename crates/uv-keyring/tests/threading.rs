#![cfg(feature = "native-auth")]

use common::{generate_random_string, init_logger};
use uv_keyring::{Entry, Error};

mod common;

#[tokio::test]
async fn test_create_then_move() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).unwrap();

    let handle = tokio::spawn(async move {
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
    });

    handle.await.expect("Task failed");
}

#[tokio::test]
async fn test_simultaneous_create_then_move() {
    init_logger();

    let mut handles = vec![];
    for i in 0..10 {
        let name = format!("{}-{}", generate_random_string(), i);
        let entry = Entry::new(&name, &name).expect("Can't create entry");

        let handle = tokio::spawn(async move {
            entry
                .set_password(&name)
                .await
                .expect("Can't set ascii password");
            let stored_password = entry
                .get_password()
                .await
                .expect("Can't get ascii password");
            assert_eq!(
                stored_password, name,
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
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task failed");
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_create_set_then_move() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let password = "test ascii password";
    entry
        .set_password(password)
        .await
        .expect("Can't set ascii password");

    let handle = tokio::spawn(async move {
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
    });

    handle.await.expect("Task failed");
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_simultaneous_create_set_then_move() {
    init_logger();

    let mut handles = vec![];
    for i in 0..10 {
        let name = format!("{}-{}", generate_random_string(), i);
        let entry = Entry::new(&name, &name).expect("Can't create entry");
        entry
            .set_password(&name)
            .await
            .expect("Can't set ascii password");

        let handle = tokio::spawn(async move {
            let stored_password = entry
                .get_password()
                .await
                .expect("Can't get ascii password");
            assert_eq!(
                stored_password, name,
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
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task failed");
    }
}

#[tokio::test]
async fn test_simultaneous_independent_create_set() {
    init_logger();

    let mut handles = vec![];
    for i in 0..10 {
        let name = format!("thread_entry{i}");
        let handle = tokio::spawn(async move {
            let entry = Entry::new(&name, &name).expect("Can't create entry");
            entry
                .set_password(&name)
                .await
                .expect("Can't set ascii password");
            let stored_password = entry
                .get_password()
                .await
                .expect("Can't get ascii password");
            assert_eq!(
                stored_password, name,
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
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task failed");
    }
}

#[tokio::test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn test_multiple_create_delete_single_thread() {
    init_logger();

    let name = generate_random_string();
    let entry = Entry::new(&name, &name).expect("Can't create entry");
    let repeats = 10;
    for _i in 0..repeats {
        entry
            .set_password(&name)
            .await
            .expect("Can't set ascii password");
        let stored_password = entry
            .get_password()
            .await
            .expect("Can't get ascii password");
        assert_eq!(
            stored_password, name,
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
}

/// Empirically, this test frequently flakes on Windows indicating that these operations are
/// not concurrency-safe.
#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_simultaneous_multiple_create_delete_single_thread() {
    init_logger();

    let mut handles = vec![];
    for t in 0..10 {
        let name = generate_random_string();
        let handle = tokio::spawn(async move {
            let name = format!("{name}-{t}");
            let entry = Entry::new(&name, &name).expect("Can't create entry");
            let repeats = 10;
            for _i in 0..repeats {
                entry
                    .set_password(&name)
                    .await
                    .expect("Can't set ascii password");
                let stored_password = entry
                    .get_password()
                    .await
                    .expect("Can't get ascii password");
                assert_eq!(
                    stored_password, name,
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
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task failed");
    }
}
