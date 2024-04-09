use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use uv_interpreter::PythonVersion;
use uv_toolchain::{DownloadMetadataError, PythonDownloadMetadata, PythonDownloadRequest};

#[derive(Parser, Debug)]
pub(crate) struct FetchPythonArgs {
    versions: Vec<String>,
}

pub(crate) async fn fetch_python(args: FetchPythonArgs) -> Result<()> {
    let versions = if args.versions.is_empty() {
        println!("Reading versions from file...");
        read_versions_file().await?
    } else {
        args.versions
    };

    let requests = versions
        .iter()
        .map(|version| match PythonDownloadRequest::from_str(version) {
            Ok(request) => request.fill(),
            err @ Err(_) => err,
        })
        .collect::<Result<Vec<_>, DownloadMetadataError>>()?;

    dbg!(&requests);

    let downloads = requests
        .iter()
        .map(
            |request| match PythonDownloadMetadata::from_request(request) {
                Some(download) => download,
                None => panic!("No download found for request {request:?}"),
            },
        )
        .collect::<Vec<_>>();

    dbg!(downloads);

    // let cache = Cache::try_from(args.cache_args)?;

    // let venv = PythonEnvironment::from_virtualenv(&cache)?;
    // let index_locations =
    //     IndexLocations::new(args.index_url, args.extra_index_url, args.find_links, false);
    // let index = InMemoryIndex::default();
    // let in_flight = InFlight::default();
    // let no_build = if args.no_build {
    //     NoBuild::All
    // } else {
    //     NoBuild::None
    // };
    // let client = RegistryClientBuilder::new(cache.clone())
    //     .index_urls(index_locations.index_urls())
    //     .build();
    // let flat_index = {
    //     let client = FlatIndexClient::new(&client, &cache);
    //     let entries = client.fetch(index_locations.flat_index()).await?;
    //     FlatIndex::from_entries(
    //         entries,
    //         venv.interpreter().tags()?,
    //         &no_build,
    //         &NoBinary::None,
    //     )
    // };
    // let config_settings = ConfigSettings::default();

    // let build_dispatch = BuildDispatch::new(
    //     &client,
    //     &cache,
    //     venv.interpreter(),
    //     &index_locations,
    //     &flat_index,
    //     &index,
    //     &in_flight,
    //     SetupPyStrategy::default(),
    //     &config_settings,
    //     BuildIsolation::Isolated,
    //     &no_build,
    //     &NoBinary::None,
    // );

    // let site_packages = SitePackages::from_executable(&venv)?;

    // // Copied from `BuildDispatch`
    // let tags = venv.interpreter().tags()?;
    // let resolver = Resolver::new(
    //     Manifest::simple(args.requirements.clone()),
    //     Options::default(),
    //     venv.interpreter().markers(),
    //     venv.interpreter(),
    //     tags,
    //     &client,
    //     &flat_index,
    //     &index,
    //     &build_dispatch,
    //     &site_packages,
    // )?;
    // let resolution_graph = resolver.resolve().await.with_context(|| {
    //     format!(
    //         "No solution found when resolving: {}",
    //         args.requirements.iter().map(ToString::to_string).join(", "),
    //     )
    // })?;

    // if let Some(graphviz) = args.graphviz {
    //     let mut writer = BufWriter::new(File::create(graphviz)?);
    //     let graphviz = Dot::with_attr_getters(
    //         resolution_graph.petgraph(),
    //         &[DotConfig::NodeNoLabel, DotConfig::EdgeNoLabel],
    //         &|_graph, edge_ref| format!("label={:?}", edge_ref.weight().to_string()),
    //         &|_graph, (_node_index, dist)| {
    //             format!("label={:?}", dist.to_string().replace("==", "\n"))
    //         },
    //     );
    //     write!(&mut writer, "{graphviz:?}")?;
    // }

    // let requirements = Resolution::from(resolution_graph).requirements();

    // match args.format {
    //     ResolveCliFormat::Compact => {
    //         println!("{}", requirements.iter().map(ToString::to_string).join(" "));
    //     }
    //     ResolveCliFormat::Expanded => {
    //         for package in requirements {
    //             println!("{}", package);
    //         }
    //     }
    // }

    Ok(())
}

async fn read_versions_file() -> Result<Vec<String>> {
    let mut file = File::open(".python-versions").await?;

    // Since the file is small, just read the whole thing into a buffer then parse
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();

    Ok(lines)
}

async fn download_and_verify() -> Result<()> {
    // Create a new instance of the reqwest Client.
    let client = Client::new();

    // Make an asynchronous GET request to the URL and fetch the response stream.
    let mut response = client.get(url).send().await?;

    // Ensure the request was successful.
    response.error_for_status_ref()?;

    // Create a SHA256 hasher instance.
    let mut hasher = Sha256::new();

    // Create a buffer to store chunks of the file as they're downloaded.
    let mut file = tokio::fs::File::create("downloaded_file").await?;

    // Stream the response body, updating the hasher and writing to the file as chunks are received.
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }

    // Finalize the hasher and obtain the result.
    let result_sha = hasher.finalize();

    // Convert the SHA256 hash result to a hexadecimal string.
    let result_sha_hex = format!("{:x}", result_sha);

    // Compare the computed SHA with the expected SHA.
    if expected_sha == result_sha_hex {
        println!("SHA256 verification successful.");
        Ok(())
    } else {
        Err("SHA256 verification failed.".into())
    }
}
