//! Pinned registry artifacts used by local test servers.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use uv_configuration::TrustedHost;
use uv_static::EnvVars;

use crate::TestContext;

struct VendorArtifact {
    filename: &'static str,
    url: &'static str,
    sha256: &'static str,
}

#[derive(Clone)]
pub(crate) struct VendorFile {
    pub(crate) filename: &'static str,
    pub(crate) sha256: &'static str,
    pub(crate) bytes: Arc<[u8]>,
}

static VENDOR_FILES: LazyLock<Vec<VendorFile>> =
    LazyLock::new(|| load_vendor_files().expect("failed to load cached vendor artifacts"));

const VENDOR_ARTIFACTS: &[VendorArtifact] = &[
    VendorArtifact {
        filename: "calver-2022.6.26-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl",
        sha256: "a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0",
    },
    VendorArtifact {
        filename: "editables-0.5-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/6b/be/0f2f4a5e8adc114a02b63d92bf8edbfa24db6fc602fca83c885af2479e0e/editables-0.5-py3-none-any.whl",
        sha256: "61e5ffa82629e0d8bfe09bc44a07db3c1ab8ed1ce78a6980732870f19b5e7d4c",
    },
    VendorArtifact {
        filename: "flit_core-3.9.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl",
        sha256: "7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301",
    },
    VendorArtifact {
        filename: "hatchling-1.20.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/87/4a/7d22a92b55809c579d8deafb0dabeb102b411a0ef9439949cccef3071527/hatchling-1.20.0-py3-none-any.whl",
        sha256: "872c63aa7e8aca85e8dba07b05c6a9b28d5a149fe00638f1a47e36930197248f",
    },
    VendorArtifact {
        filename: "packaging-23.2-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/ec/1a/610693ac4ee14fcdf2d9bf3c493370e4f2ef7ae2e19217d7a237ff42367d/packaging-23.2-py3-none-any.whl",
        sha256: "8c491190033a9af7e1d931d0b5dacc2ef47509b34dd0de67ed209b5203fc88c7",
    },
    VendorArtifact {
        filename: "packaging-23.2.tar.gz",
        url: "https://files.pythonhosted.org/packages/fb/2b/9b9c33ffed44ee921d0967086d653047286054117d584f1b1a7c22ceaf7b/packaging-23.2.tar.gz",
        sha256: "048fb0e9405036518eaaf48a55953c750c11e1a1b68e0dd1a9d62ed0c092cfc5",
    },
    VendorArtifact {
        filename: "pathspec-0.12.1-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl",
        sha256: "a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08",
    },
    VendorArtifact {
        filename: "pip-24.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/8a/6a/19e9fe04fca059ccf770861c7d5721ab4c2aebc539889e97c7977528a53b/pip-24.0-py3-none-any.whl",
        sha256: "ba0d021a166865d2265246961bec0152ff124de910c5cc39f1156ce3fa7c69dc",
    },
    VendorArtifact {
        filename: "pluggy-1.3.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/05/b8/42ed91898d4784546c5f06c60506400548db3f7a4b3fb441cba4e5c17952/pluggy-1.3.0-py3-none-any.whl",
        sha256: "d89c696a773f8bd377d18e5ecda92b7a3793cbe66c87060a6fb58c7b6e1061f7",
    },
    VendorArtifact {
        filename: "setuptools-69.0.2-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/bb/e1/ed2dd0850446b8697ad28d118df885ad04140c64ace06c4bd559f7c8a94f/setuptools-69.0.2-py3-none-any.whl",
        sha256: "1e8fdff6797d3865f37397be788a4e3cba233608e9b509382a2777d25ebde7f2",
    },
    VendorArtifact {
        filename: "setuptools_scm-8.0.4-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl",
        sha256: "b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f",
    },
    VendorArtifact {
        filename: "tomli-2.0.1-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl",
        sha256: "939de3e7a6161af0c887ef91b7d41a53e7c5a1ca976325f429cb46ea9bc30ecc",
    },
    VendorArtifact {
        filename: "trove_classifiers-2023.11.29-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/f8/50/e223fe762fe21fefb7a3f37c10e9693ea5f63cb54b5ae39daa876b780abc/trove_classifiers-2023.11.29-py3-none-any.whl",
        sha256: "02307750cbbac2b3d13078662f8a5bf077732bf506e9c33c97204b7f68f3699e",
    },
    VendorArtifact {
        filename: "typing_extensions-4.9.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/b7/f4/6a90020cd2d93349b442bfcb657d0dc91eee65491600b2cb1d388bc98e6b/typing_extensions-4.9.0-py3-none-any.whl",
        sha256: "af72aea155e91adfc61c3ae9e0e342dbc0cba726d6cba4b6c72c1f34e47291cd",
    },
    VendorArtifact {
        filename: "wheel-0.42.0-py3-none-any.whl",
        url: "https://files.pythonhosted.org/packages/c7/c3/55076fc728723ef927521abaa1955213d094933dc36d4a2008d5101e1af5/wheel-0.42.0-py3-none-any.whl",
        sha256: "177f9c9b0d45c47873b619f5b650346d632cdc35fb5e4d25058e09c9e581433d",
    },
];

pub(crate) fn vendor_files() -> &'static [VendorFile] {
    &VENDOR_FILES
}

fn vendor_cache_dir() -> PathBuf {
    TestContext::test_bucket_dir().join("vendor")
}

#[tokio::main(flavor = "current_thread")]
async fn load_vendor_files() -> Result<Vec<VendorFile>> {
    let cache_dir = vendor_cache_dir();
    fs_err::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create vendor cache at `{}`", cache_dir.display()))?;

    let trusted_hosts = std::env::var(EnvVars::UV_INSECURE_HOST)
        .unwrap_or_default()
        .split(' ')
        .filter(|host| !host.is_empty())
        .map(TrustedHost::from_str)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let client = uv_client::BaseClientBuilder::default()
        .allow_insecure_host(trusted_hosts)
        .build()
        .context("failed to build vendor artifact client")?;

    let mut files = Vec::with_capacity(VENDOR_ARTIFACTS.len());
    for artifact in VENDOR_ARTIFACTS {
        let path = cache_dir.join(artifact.filename);
        ensure_cached_artifact(&client, artifact, &path).await?;
        let bytes = fs_err::read(&path).with_context(|| {
            format!("failed to read cached vendor artifact `{}`", path.display())
        })?;
        verify_bytes(artifact, &bytes)?;
        files.push(VendorFile {
            filename: artifact.filename,
            sha256: artifact.sha256,
            bytes: bytes.into(),
        });
    }

    Ok(files)
}

async fn ensure_cached_artifact(
    client: &uv_client::BaseClient,
    artifact: &VendorArtifact,
    path: &Path,
) -> Result<()> {
    if let Ok(bytes) = fs_err::read(path)
        && verify_bytes(artifact, &bytes).is_ok()
    {
        return Ok(());
    }

    let url = artifact
        .url
        .parse()
        .with_context(|| format!("invalid vendor artifact URL `{}`", artifact.url))?;
    let response = client
        .for_host(&url)
        .get(reqwest::Url::from(url))
        .send()
        .await
        .with_context(|| format!("failed to download `{}`", artifact.url))?
        .error_for_status()
        .with_context(|| format!("failed to download `{}`", artifact.url))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read `{}`", artifact.url))?;
    verify_bytes(artifact, &bytes)?;

    let parent = path
        .parent()
        .context("vendor artifact cache path should have a parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create vendor temp file in `{}`",
            parent.display()
        )
    })?;
    temp.write_all(&bytes)
        .with_context(|| format!("failed to write `{}`", artifact.filename))?;
    temp.as_file_mut()
        .sync_all()
        .with_context(|| format!("failed to sync `{}`", artifact.filename))?;

    match temp.persist(path) {
        Ok(_) => Ok(()),
        Err(error) => {
            if let Ok(bytes) = fs_err::read(path)
                && verify_bytes(artifact, &bytes).is_ok()
            {
                Ok(())
            } else {
                Err(error.error).with_context(|| {
                    format!(
                        "failed to persist cached vendor artifact `{}`",
                        path.display()
                    )
                })
            }
        }
    }
}

fn verify_bytes(artifact: &VendorArtifact, bytes: &[u8]) -> Result<()> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == artifact.sha256 {
        Ok(())
    } else {
        bail!(
            "hash mismatch for cached vendor artifact `{}`: expected `{}`, found `{}`",
            artifact.filename,
            artifact.sha256,
            actual
        )
    }
}
