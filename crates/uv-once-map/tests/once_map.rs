use std::error::Error;
use std::pin::pin;

use futures::poll;
use uv_once_map::{OnceMap, Registration};

#[tokio::test]
async fn registered_waiters() -> Result<(), Box<dyn Error>> {
    let map = OnceMap::<_, _>::default();
    let Registration::New(first) = map.register_entry("package") else {
        return Err("expected a new registration".into());
    };
    let Registration::Existing(second) = map.register_entry("package") else {
        return Err("expected the existing registration".into());
    };
    let adopted = map.entry(&"package").ok_or("missing registration")?;
    let cloned = first.clone();

    let mut first_wait = pin!(first.wait());
    let mut second_wait = pin!(second.wait());
    assert!(poll!(&mut first_wait).is_pending());
    assert!(poll!(&mut second_wait).is_pending());
    assert_eq!(map.get(&"package"), None);

    map.done("package", 42);
    assert_eq!(first_wait.await, 42);
    assert_eq!(second_wait.await, 42);
    assert_eq!(adopted.wait().await, 42);
    assert_eq!(cloned.wait_blocking(), 42);
    assert_eq!(first.wait().await, 42);
    assert_eq!(map.wait(&"package").await?, 42);
    Ok(())
}

#[test]
fn removed_entry_retains_result() -> Result<(), Box<dyn Error>> {
    let map = OnceMap::<_, _>::default();
    assert!(map.entry(&"package").is_none());
    assert!(map.register("package"));
    assert!(!map.register("package"));
    let entry = map.entry(&"package").ok_or("missing registration")?;

    map.done("package", 42);
    assert_eq!(map.remove(&"package"), Some(42));
    assert!(map.entry(&"package").is_none());
    assert!(map.register("package"));
    map.done("package", 43);
    let replacement = map.entry(&"package").ok_or("missing replacement")?;
    drop(map);

    assert_eq!(entry.wait_blocking(), 42);
    assert_eq!(replacement.wait_blocking(), 43);
    Ok(())
}

#[tokio::test]
async fn preloaded_and_legacy_entries() -> Result<(), Box<dyn Error>> {
    let map = OnceMap::<_, _>::from_iter([("preloaded", 42)]);
    let entry = map.entry(&"preloaded").ok_or("missing preloaded entry")?;
    assert_eq!(entry.wait().await, 42);
    assert!(!map.register("preloaded"));

    assert_eq!(map.register_or_wait(&"running").await, None);
    let mut wait = pin!(map.register_or_wait(&"running"));
    assert!(poll!(&mut wait).is_pending());
    map.done("running", 43);
    assert_eq!(wait.await, Some(43));
    assert_eq!(map.register_or_wait(&"running").await, Some(43));

    map.done("unregistered", 44);
    assert_eq!(map.wait_blocking(&"unregistered")?, 44);
    map.done("unregistered", 45);
    assert_eq!(map.get(&"unregistered"), Some(45));
    Ok(())
}
