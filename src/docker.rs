use anyhow::{anyhow,Context,Result};
use semver::Version;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct Options {
    pub skip_errors: bool,
    pub dry_run: bool,
    pub force: bool,
    pub include_latest: bool,
}

#[derive(Debug)]
pub struct DockerImage {
    repository: String,
    image: String,
}

impl DockerImage {
    pub fn from(string: &String) -> DockerImage {
        let split = string.split("/");
        let parts: Vec<&str> = split.collect();

        DockerImage {
            repository: String::from(parts[0]),
            image: String::from(parts[1]),
        }
    }

    fn latest_synced_image(&self, directory: &String, options: &Options) -> Result<String> {
        let dir = Path::new(directory).join(self.repository.clone());

        if !dir.is_dir() {
            if !options.force {
                return Err(anyhow!("{:#?} is not a directory", dir));
            }

            fs::create_dir(&dir).context("Could not create directory")?;
        }

        let latest = String::new();
        let version = Version::parse("0.0.0");
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            println!("{:#?}", path);
        }

        Ok(latest)
    }

    pub fn sync(&self, directory: &String, options: &Options) -> Result<()> {
        self.latest_synced_image(directory, options)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from() {
        let base = String::from("foo/bar");
        let docker_image = DockerImage::from(&base);

        assert_eq!(docker_image.base, base);
        assert_eq!(docker_image.repository, String::from("foo"));
        assert_eq!(docker_image.image, String::from("bar"));
    }
}
