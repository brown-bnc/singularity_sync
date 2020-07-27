mod docker;

use anyhow::{Context, Result};
use clap::{App, Arg};
use reqwest;
use serde::Deserialize;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use docker::{DockerImage, Options};

#[derive(Deserialize, Debug)]
struct Manifest {
    docker: Vec<String>,
}

fn manifest_from_stdin(manifest: &mut String) -> Result<()> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_string(manifest)?;
    Ok(())
}

fn manifest_from_file(manifest_path: &String, manifest: &mut String) -> Result<()> {
    let mut file = File::open(manifest_path)?;
    file.read_to_string(manifest)?;
    Ok(())
}

fn manifest_from_url(manifest_path: &String, manifest: &mut String) -> Result<()> {
    let mut response = reqwest::blocking::get(manifest_path)?;
    response.read_to_string(manifest)?;
    Ok(())
}

fn parse_manifest(manifest_path: Option<&str>, manifest: &mut String) -> Result<Manifest> {
    if manifest_path.is_none() {
        manifest_from_stdin(manifest)?;
    } else {
        let manifest_path = String::from(manifest_path.unwrap());

        if Path::new(&manifest_path).exists() {
            manifest_from_file(&manifest_path, manifest)?
        } else {
            manifest_from_url(&manifest_path, manifest)?;
        }
    }

    Ok(serde_yaml::from_str(manifest)?)
}

fn sync_from_manifest(directory: &String, manifest: &Manifest, options: &Options) -> Result<()> {
    for image in &manifest.docker {
        DockerImage::from(image).sync(directory, options)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let matches = App::new("singularity-sync")
        .about("Syncs singularity containers")
        .author("Bradford N. Roarr")
        .arg(
            Arg::with_name("DIR")
                .help("Directory to sync singularity containers to")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("manifest")
                .short("m")
                .long("manifest")
                .value_name("FILE")
                .help("Manifest to use for syncing")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("skip_errors")
                .short("s")
                .long("skip-errors")
                .help("Continue processing on error"),
        )
        .arg(
            Arg::with_name("dry_run")
                .short("d")
                .long("dry-run")
                .help("Do not build singularity containers"),
        )
        .arg(
            Arg::with_name("force")
                .short("f")
                .long("force")
                .help("Overwrite any existing singularity containers"),
        )
        .arg(
            Arg::with_name("include_latest")
                .short("l")
                .long("include-latest")
                .help("Include the \"latest\" image"),
        )
        .version("v0.2.0")
        .get_matches();

    let manifest_path = matches.value_of("manifest");
    let mut manifest = String::new();
    let manifest =
        parse_manifest(manifest_path, &mut manifest).context("Failed to parse manifest")?;

    let directory = String::from(matches.value_of("DIR").unwrap());
    let options = Options {
        skip_errors: matches.is_present("skip_errors"),
        dry_run: matches.is_present("dry_run"),
        force: matches.is_present("force"),
        include_latest: matches.is_present("include_latest"),
    };
    sync_from_manifest(&directory, &manifest, &options)?;

    Ok(())
}
