use std::error::Error;
use std::pin::pin;
use std::sync::Arc;

use futures::poll;
use uv_once_map::{RegisteredOnceMap, Registration};

#[tokio::test]
async fn registered_waiters() -> Result<(), Box<dyn Error>> {
    let mut map = RegisteredOnceMap::<_, _>::default();
    assert!(map.get_registered("package").is_none());
    {
        let Registration::New(first) = map.register_entry("package") else {
            return Err("lookup must not register an absent key".into());
        };
        let Registration::Existing(second) = map.register_entry("package") else {
            return Err("expected the existing registration".into());
        };
        let cloned = first.clone();
        let mut first_wait = pin!(first.wait());
        let mut second_wait = pin!(second.wait());
        assert!(poll!(&mut first_wait).is_pending());
        assert!(poll!(&mut second_wait).is_pending());

        map.done("package", 42);
        assert_eq!(first_wait.await, 42);
        assert_eq!(second_wait.await, 42);
        assert_eq!(cloned.wait_blocking(), 42);
        assert_eq!(first.wait().await, 42);
    }
    assert_eq!(map.remove(&"package"), Some(42));
    assert!(map.get_registered("package").is_none());
    assert!(map.register("package"));
    Ok(())
}

#[test]
fn preloaded_entry() -> Result<(), Box<dyn Error>> {
    let mut map = RegisteredOnceMap::<_, _>::from_iter([("package", Arc::new(42))]);
    {
        let entry = map
            .get_registered("package")
            .ok_or("missing preloaded entry")?;
        assert_eq!(entry.key(), &"package");
        assert_eq!(*entry.wait_blocking(), 42);
    }
    let result = map.remove(&"package").ok_or("missing result")?;
    assert_eq!(*result, 42);
    assert_eq!(Arc::strong_count(&result), 1);
    Ok(())
}

#[tokio::test]
async fn register_or_wait() {
    let map = RegisteredOnceMap::<_, _>::default();
    assert_eq!(map.register_or_wait(&"package").await, None);
    let mut wait = pin!(map.register_or_wait(&"package"));
    assert!(poll!(&mut wait).is_pending());
    map.done("package", 42);
    assert_eq!(wait.await, Some(42));
    assert_eq!(map.register_or_wait(&"package").await, Some(42));
}
