use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::{App, Arg};
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::SystemTime;

#[derive(Debug)]
struct Options {
    dry_run: bool,
    first_sync: usize,
    force: bool,
    parallelize: bool,
}

#[derive(Deserialize, Debug)]
struct Manifest {
    docker: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Tag {
    name: String,
    last_updated: DateTime<Utc>,
}

#[derive(Deserialize, Debug)]
struct TagResponse {
    count: u32,
    next: Option<String>,
    previous: Option<String>,
    results: Vec<Tag>,
}

fn manifest_from_stdin(manifest: &mut String) -> Result<()> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_string(manifest)?;
    Ok(())
}

fn manifest_from_file(manifest_path: &str, manifest: &mut String) -> Result<()> {
    let mut file = File::open(manifest_path)?;
    file.read_to_string(manifest)?;
    Ok(())
}

fn manifest_from_url(manifest_path: &str, manifest: &mut String) -> Result<()> {
    let mut response = reqwest::blocking::get(manifest_path)?;
    response.read_to_string(manifest)?;
    Ok(())
}

fn parse_manifest(manifest_path: Option<&str>) -> Result<Manifest> {
    let mut manifest = String::new();

    if let Some(manifest_path) = manifest_path {
        let manifest_path = String::from(manifest_path);

        if Path::new(&manifest_path).exists() {
            manifest_from_file(&manifest_path, &mut manifest)?
        } else {
            manifest_from_url(&manifest_path, &mut manifest)?;
        }
    } else {
        manifest_from_stdin(&mut manifest)?;
    }

    Ok(serde_yaml::from_str(&manifest)?)
}

fn lastest_sync_timestamp(dir: &Path, image: &str) -> Result<DateTime<Utc>> {
    let latest_sync = fs::read_dir(&dir)?
        .filter_map(|entry| {
            // Ignore errors coming from read_dir
            if entry.is_err() {
                return None;
            }

            // If the path is not a file ignore it
            let path = entry.unwrap().path();
            if !path.is_file() {
                return None;
            }

            // If the file has no extension ignore it
            // If the file extension is not "sif" ignore it
            let extension = path.extension()?;
            if extension != "sif" {
                return None;
            }

            // If the file name cannot be converted to a string ignore it
            let file_name = path.file_name()?.to_os_string().into_string();
            if file_name.is_err() {
                return None;
            }

            // If the filename doesn't match image ignore it
            let file_name = file_name.unwrap();
            if !file_name.contains(image) {
                return None;
            }

            // If the file cannot be stat'd ignore it
            let metadata = fs::metadata(path);
            if metadata.is_err() {
                return None;
            }

            // If the fs does not implement modified time ignore it
            let last_modified = metadata.unwrap().modified();
            if last_modified.is_err() {
                return None;
            }

            // NOTE (BNR): I had a check in here to skip latest, but since we're
            //             using the last modified timestamp of the sif images
            //             we can use the latest image. It'll just give us the
            //             time we should look for stuff after.

            Some(last_modified.unwrap())
        })
        .fold(SystemTime::UNIX_EPOCH, |prev, curr| {
            if prev < curr {
                curr
            } else {
                prev
            }
        });

    Ok(DateTime::from(latest_sync))
}

fn tags_after_timestamp(
    repository: &str,
    image: &str,
    latest_sync: DateTime<Utc>,
) -> Result<Vec<String>> {
    let mut url = format!(
        "https://registry.hub.docker.com/v2/repositories/{}/{}/tags",
        repository, image
    );
    let mut tags: Vec<String> = Vec::new();

    loop {
        let response = reqwest::blocking::get(&url)?;
        let response = response.text()?;
        let response: TagResponse = serde_json::from_str(&response)?;

        response.results.iter().for_each(|tag| {
            if tag.last_updated > latest_sync {
                tags.push(tag.name.clone());
            }
        });

        match response.next {
            Some(next) => url = next,
            _ => break,
        }
    }

    Ok(tags)
}

fn slurm_sync_command(
    directory: &str,
    repository: &str,
    image: &str,
    tag: &str,
    options: &Options,
) -> Result<()> {
    let sif_path = format!("{}/{}/{}-{}.sif", directory, repository, image, tag);
    let docker_uri = format!("docker://{}/{}:{}", repository, image, tag);
    let job_name = format!("SingularitySync-{}-{}", repository, image);
    let output = format!(
        "/gpfs/scratch/%u/SingularitySync-{}-{}-%j.out",
        repository, image
    );
    let force = if options.force { "-F" } else { "" };

    let memory = option_env!("SBATCH_MEM_PER_NODE");
    let memory = match memory {
        Some(m) => m,
        None => "8G",
    };

    let time = option_env!("SBATCH_TIMELIMIT");
    let time = match time {
        Some(t) => t,
        None => "8:00:00",
    };

    let singularity_cachedir = option_env!("SINGULARITY_CACHEDIR");
    let singularity_cachedir = match singularity_cachedir {
        Some(x) => x,
        None => "${HOME}/scratch/singularity",
    };

    let singularity_tmpdir = option_env!("SINGULARITY_TMPDIR");
    let singularity_tmpdir = match singularity_tmpdir {
        Some(x) => x,
        None => "${HOME}/scratch/tmp",
    };

    let script = [
        String::from("#!/usr/bin/env bash\n"),
        format!("#SBATCH --time={}\n", time),
        format!("#SBATCH --mem={}\n", memory),
        format!("#SBATCH -J {}\n", job_name),
        format!("#SBATCH -o {}\n", output),
        format!("export SINGULARITY_CACHEDIR={}\n", singularity_cachedir),
        format!("export SINGULARITY_TMPDIR={}\n", singularity_tmpdir),
        format!("singularity build {} {} {}", force, sif_path, docker_uri),
    ]
    .concat();

    if options.dry_run {
        println!("sbatch <<EOF");
        println!("{}", script);
        println!("EOF");
        return Ok(());
    }

    let command = Command::new("sbatch").stdin(Stdio::piped()).spawn()?;
    command
        .stdin
        .context("Could not get handle to stdin")?
        .write_all(script.as_bytes())?;

    Ok(())
}

fn run_sync_command(
    directory: &str,
    repository: &str,
    image: &str,
    tag: &str,
    options: &Options,
) -> Result<()> {
    if options.parallelize {
        slurm_sync_command(directory, repository, image, tag, options)?;
        return Ok(());
    }

    let sif_path = format!("{}/{}/{}-{}.sif", directory, repository, image, tag);
    let docker_uri = format!("docker://{}/{}:{}", repository, image, tag);

    if options.dry_run {
        let force = if options.force { "-F" } else { "" };
        let sbatch_cmd = format!("singularity build {} {} {}", force, sif_path, docker_uri);
        println!("{}", sbatch_cmd);
        return Ok(());
    }

    let mut command = Command::new("singularity");

    command.arg("build");

    if options.force {
        command.arg("-F");
    }

    command.arg(sif_path).arg(docker_uri).status()?;

    Ok(())
}

fn sync_docker_image(directory: &str, image: &str, options: &Options) -> Result<()> {
    let image_split: Vec<&str> = image.rsplit('/').collect();
    let image = String::from(image_split[0]);
    let repository = String::from(image_split[1]);
    let image_dir = Path::new(directory).join(repository.clone());

    if !image_dir.is_dir() {
        if options.force {
            fs::create_dir(&image_dir).context("Could not create directory")?;
        } else {
            return Err(anyhow!("Image directory not found: {:#?}", image_dir));
        }
    }

    let latest_sync = lastest_sync_timestamp(&image_dir, &image)?;
    let tags_to_sync = tags_after_timestamp(&repository, &image, latest_sync)?;

    let epoch: DateTime<Utc> = DateTime::from(SystemTime::UNIX_EPOCH);
    let tags_to_sync = if latest_sync == epoch {
        &tags_to_sync[0..options.first_sync]
    } else {
        tags_to_sync.as_slice()
    };

    for tag in tags_to_sync {
        run_sync_command(directory, &repository, &image, &tag, options)?;
    }

    Ok(())
}

fn sync_manifest(directory: &str, manifest: &Manifest, options: &Options) -> Result<()> {
    for image in &manifest.docker {
        sync_docker_image(directory, image, options)?;
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
            Arg::with_name("first_sync")
                .short("F")
                .long("first-sync")
                .default_value("5")
                .help("The number of tags to pull on first sync"),
        )
        .arg(
            Arg::with_name("parallelize")
                .short("p")
                .long("parallelize")
                .help("Parallelize using Slurm scheduler"),
        )
        .version("v0.2.0")
        .get_matches();

    let manifest_path = matches.value_of("manifest");
    let manifest = parse_manifest(manifest_path).context("Failed to parse manifest")?;

    let directory = String::from(matches.value_of("DIR").unwrap());
    let options = Options {
        dry_run: matches.is_present("dry_run"),
        first_sync: matches.value_of("first_sync").unwrap().parse()?,
        force: matches.is_present("force"),
        parallelize: matches.is_present("parallelize"),
    };
    sync_manifest(&directory, &manifest, &options)?;

    Ok(())
}
