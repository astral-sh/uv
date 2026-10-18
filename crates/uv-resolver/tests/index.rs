use std::error::Error;
use std::sync::Arc;

use uv_distribution_types::DistributionId;
use uv_resolver::InMemoryIndex;
use uv_resolver_types::{MetadataResponse, MetadataUnavailable};

#[test]
fn invalidation_requires_exclusive_index() -> Result<(), Box<dyn Error>> {
    let mut index = InMemoryIndex::default();
    let id = DistributionId::AbsoluteUrl("https://example.com/package.whl".to_string());
    index.distributions().done(
        id.clone(),
        Arc::new(MetadataResponse::Unavailable(MetadataUnavailable::Offline)),
    );
    {
        let shared = index.clone();
        let entry = shared
            .distributions()
            .get_registered(id.clone())
            .ok_or("missing registration")?;
        assert!(index.distributions_mut().is_none());
        let response = entry.wait_blocking();
        assert!(Arc::ptr_eq(
            &response,
            &index.distributions().get(&id).ok_or("missing result")?
        ));
    }
    let metadata = index
        .distributions_mut()
        .ok_or("index should be exclusively owned")?;
    assert!(metadata.remove(&id).is_some());
    assert!(index.distributions().get_registered(id).is_none());
    Ok(())
}
