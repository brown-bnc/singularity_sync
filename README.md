# Singularity Sync (Rust Edition)

The Singularity Sync utility rebuilds Docker containers as Singularity containers. It does so by parsing a manifest file containing a list of Docker images. Below is an example of a manifest file. 

```
---
docker:
- bids/validator
- brownbnc/xnat-tools
- poldracklab/fmriprep
- poldracklab/mriqc
```

As of this writing (2020/08/26) there is only support for images from the official [Docker Hub](https://hub.docker.com). The repositories are represented in `${org}/${respository}` form.

## Install

To install Singularity Sync [install rust](https://www.rust-lang.org/tools/install).

Build using cargo:

```
cargo build --release
```

Run the resulting executable:

```
./target/release/singularity_sync
```

## Development

Development follows a standard rust workflow. Most everything is done through cargo. 

Build development version with cargo:

```
cargo build
```

Run development version:

```
cargo run -- -h
```

Format with cargo:

```
cargo fmt
```

Lint with cargo:

```
cargo clippy
```

## Future work

* Parallelize builds using [Slurm](https://slurm.schedmd.com/)
* Support single Docker images without a manifest
* Support Docker registries other than [Docker Hub](https://hub.docker.com)
* Support native Singularity builds
* Add real tests
