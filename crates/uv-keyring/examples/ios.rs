use uv_keyring::{Entry, Error};

#[unsafe(no_mangle)]
extern "C" fn test() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        test_invalid_parameter();
        test_empty_keyring().await;
        test_empty_password_input().await;
        test_round_trip_ascii_password().await;
        test_round_trip_non_ascii_password().await;
        test_update_password().await;
    });
}

fn test_invalid_parameter() {
    let entry = Entry::new("", "user");
    assert!(
        matches!(entry, Err(Error::Invalid(_, _))),
        "Created entry with empty service"
    );
    let entry = Entry::new("service", "");
    assert!(
        matches!(entry, Err(Error::Invalid(_, _))),
        "Created entry with empty user"
    );
    let entry = Entry::new_with_target("test", "service", "user");
    assert!(
        matches!(entry, Err(Error::Invalid(_, _))),
        "Created entry with non-default target"
    );
}

async fn test_empty_keyring() {
    let name = "test_empty_keyring".to_string();
    let entry = Entry::new(&name, &name).expect("Failed to create entry");
    assert!(matches!(entry.get_password().await, Err(Error::NoEntry)))
}

async fn test_empty_password_input() {
    let name = "test_empty_password_input".to_string();
    let entry = Entry::new(&name, &name).expect("Failed to create entry");
    let in_pass = "";
    entry
        .set_password(in_pass)
        .await
        .expect("Couldn't set empty password");
    let out_pass = entry
        .get_password()
        .await
        .expect("Couldn't get empty password");
    assert_eq!(in_pass, out_pass);
    entry
        .delete_credential()
        .await
        .expect("Couldn't delete credential with empty password");
    assert!(
        matches!(entry.get_password().await, Err(Error::NoEntry)),
        "Able to read a deleted password"
    )
}

async fn test_round_trip_ascii_password() {
    let name = "test_round_trip_ascii_password".to_string();
    let entry = Entry::new(&name, &name).expect("Failed to create entry");
    let password = "test ascii password";
    entry.set_password(password).await.unwrap();
    let stored_password = entry.get_password().await.unwrap();
    assert_eq!(stored_password, password);
    entry.delete_credential().await.unwrap();
    assert!(matches!(entry.get_password().await, Err(Error::NoEntry)))
}

async fn test_round_trip_non_ascii_password() {
    let name = "test_round_trip_non_ascii_password".to_string();
    let entry = Entry::new(&name, &name).expect("Failed to create entry");
    let password = "このきれいな花は桜です";
    entry.set_password(password).await.unwrap();
    let stored_password = entry.get_password().await.unwrap();
    assert_eq!(stored_password, password);
    entry.delete_credential().await.unwrap();
    assert!(matches!(entry.get_password().await, Err(Error::NoEntry)))
}

async fn test_update_password() {
    let name = "test_update_password".to_string();
    let entry = Entry::new(&name, &name).expect("Failed to create entry");
    let password = "test ascii password";
    entry.set_password(password).await.unwrap();
    let stored_password = entry.get_password().await.unwrap();
    assert_eq!(stored_password, password);
    let password = "このきれいな花は桜です";
    entry.set_password(password).await.unwrap();
    let stored_password = entry.get_password().await.unwrap();
    assert_eq!(stored_password, password);
    entry.delete_credential().await.unwrap();
    assert!(matches!(entry.get_password().await, Err(Error::NoEntry)))
}
