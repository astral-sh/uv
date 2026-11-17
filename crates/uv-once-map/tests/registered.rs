use std::error::Error;
use std::future::pending;
use std::io;
use std::pin::pin;
use std::sync::Arc;

use futures::{FutureExt, poll};
use uv_once_map::RegisteredOnceMap;

#[tokio::test]
async fn registered_waiters() -> Result<(), Box<dyn Error>> {
    let mut map = RegisteredOnceMap::<_, _>::default();
    assert!(map.get_registered("package").is_none());
    {
        let (first, completion) = map.register_entry("package");
        let completion = completion.ok_or("lookup must not register an absent key")?;
        let (second, other) = map.register_entry("package");
        assert!(other.is_none());
        let cloned = first.clone();
        let mut first_wait = pin!(first.wait());
        let mut second_wait = pin!(second.wait());
        assert!(poll!(&mut first_wait).is_pending());
        assert!(poll!(&mut second_wait).is_pending());

        completion.finish(42);
        assert_eq!(first_wait.await, Some(42));
        assert_eq!(second_wait.await, Some(42));
        assert_eq!(cloned.wait_blocking(), Some(42));
        assert_eq!(first.wait().await, Some(42));
    }
    assert_eq!(map.remove(&"package"), Some(42));
    assert!(map.get_registered("package").is_none());
    assert!(map.register("package").is_some());
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
        assert_eq!(*entry.wait_blocking().ok_or("abandoned registration")?, 42);
    }
    let result = map.remove(&"package").ok_or("missing result")?;
    assert_eq!(*result, 42);
    assert_eq!(Arc::strong_count(&result), 1);
    Ok(())
}

#[tokio::test]
async fn abandoned_registration() -> Result<(), Box<dyn Error>> {
    let map = RegisteredOnceMap::<_, _>::default();
    let (first, producer) = map.register_entry("package");
    let mut waiter = pin!(first.wait());
    assert!(poll!(&mut waiter).is_pending());

    drop(producer.ok_or("expected a new registration")?);
    let (retry, producer) = map.register_entry("package");
    producer
        .ok_or("expected to retry an abandoned request")?
        .finish(42);
    assert_eq!(waiter.await, None);
    assert_eq!(first.wait_blocking(), None);
    assert_eq!(retry.wait_blocking(), Some(42));
    Ok(())
}

#[tokio::test]
async fn seed_pending_request() -> Result<(), Box<dyn Error>> {
    let map = RegisteredOnceMap::<_, _>::default();
    let (entry, completion) = map.register_entry("package");
    let mut waiter = pin!(entry.wait());
    assert!(poll!(&mut waiter).is_pending());
    map.insert("package", 42);
    assert_eq!(waiter.await, Some(42));
    completion.ok_or("expected a new registration")?.finish(99);
    assert_eq!(map.get(&"package"), Some(42));
    map.insert("package", 43);
    assert_eq!(entry.wait_blocking(), Some(43));
    Ok(())
}

/// An input-preparation task waiting on an abandoned fetch can run its own initializer.
#[tokio::test]
async fn initialize_after_producer_drops() -> Result<(), Box<dyn Error>> {
    let map = RegisteredOnceMap::<_, _>::default();
    let mut producer = Box::pin(map.get_or_try_init("package", async || {
        pending::<Result<i32, io::Error>>().await
    }));
    assert!(poll!(&mut producer).is_pending());
    let mut init = pin!(map.get_or_try_init("package", async || Ok::<_, io::Error>(42)));
    assert!(poll!(&mut init).is_pending());
    drop(producer);
    assert_eq!(
        init.now_or_never().ok_or("initializer stayed pending")??,
        42
    );
    assert_eq!(map.get(&"package"), Some(42));
    Ok(())
}
