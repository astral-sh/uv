use crate::{HashError, Hashes};

#[test]
fn parse_hashes() -> Result<(), HashError> {
    let hashes: Hashes =
        "sha512:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
    assert_eq!(
        hashes,
        Hashes {
            md5: None,
            sha256: None,
            sha384: None,
            sha512: Some("40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()),
        }
    );

    let hashes: Hashes =
        "sha384:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
    assert_eq!(
        hashes,
        Hashes {
            md5: None,
            sha256: None,
            sha384: Some("40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()),
            sha512: None
        }
    );

    let hashes: Hashes =
        "sha256:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
    assert_eq!(
        hashes,
        Hashes {
            md5: None,
            sha256: Some("40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()),
            sha384: None,
            sha512: None
        }
    );

    let hashes: Hashes =
        "md5:090376d812fb6ac5f171e5938e82e7f2d7adc2b629101cec0db8b267815c85e2".parse()?;
    assert_eq!(
        hashes,
        Hashes {
            md5: Some("090376d812fb6ac5f171e5938e82e7f2d7adc2b629101cec0db8b267815c85e2".into()),
            sha256: None,
            sha384: None,
            sha512: None
        }
    );

    let result =
        "sha256=40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse::<Hashes>();
    assert!(result.is_err());

    let result =
        "blake2:55f44b440d491028addb3b88f72207d71eeebfb7b5dbf0643f7c023ae1fba619".parse::<Hashes>();
    assert!(result.is_err());

    Ok(())
}
