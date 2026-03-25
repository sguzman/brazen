use std::path::{Path, PathBuf};

pub const SERVO_SOURCE_ENV: &str = "BRAZEN_SERVO_SOURCE";

#[derive(Debug, Clone)]
pub struct ResourceDirResolution {
    pub path: PathBuf,
    pub source: ResourceDirSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceDirSource {
    Config,
    EnvVar,
    Vendor,
}

const REQUIRED_RESOURCES: [&str; 10] = [
    "gatt_blocklist.txt",
    "public_domains.txt",
    "hsts_preload.fstmap",
    "badcert.html",
    "neterror.html",
    "rippy.png",
    "crash.html",
    "directory-listing.html",
    "about-memory.html",
    "debugger.js",
];

pub fn resolve_resource_dir(
    config_path: Option<&str>,
    servo_source_env: Option<&str>,
) -> Result<ResourceDirResolution, String> {
    if let Some(value) = config_path {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("engine.servo_resources_dir is empty".to_string());
        }
        let path = resolve_path(trimmed)?;
        return ensure_dir(path, ResourceDirSource::Config);
    }

    if let Some(env_value) = servo_source_env {
        let trimmed = env_value.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed).join("resources");
            if path.is_dir() {
                return ensure_dir(path, ResourceDirSource::EnvVar);
            }
        }
    }

    let vendor_path = PathBuf::from("vendor").join("servo").join("resources");
    if vendor_path.is_dir() {
        return ensure_dir(vendor_path, ResourceDirSource::Vendor);
    }

    Err("could not resolve Servo resources directory".to_string())
}

fn resolve_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        let cwd = std::env::current_dir()
            .map_err(|error| format!("failed to resolve current dir: {error}"))?;
        Ok(cwd.join(path))
    }
}

fn ensure_dir(path: PathBuf, source: ResourceDirSource) -> Result<ResourceDirResolution, String> {
    if path.is_dir() {
        validate_resources(&path)?;
        Ok(ResourceDirResolution { path, source })
    } else {
        Err(format!(
            "resources directory not found at {}",
            path.display()
        ))
    }
}

fn validate_resources(path: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    for name in REQUIRED_RESOURCES {
        let file_path = path.join(name);
        if !file_path.is_file() {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "resources directory missing files at {}: {}",
            path.display(),
            missing.join(", ")
        ))
    }
}

#[cfg(feature = "servo-upstream")]
pub struct ServoResourceReader {
    base_dir: PathBuf,
}

#[cfg(feature = "servo-upstream")]
impl ServoResourceReader {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

#[cfg(feature = "servo-upstream")]
impl libservo::resources::ResourceReaderMethods for ServoResourceReader {
    fn read(&self, resource: libservo::resources::Resource) -> Vec<u8> {
        let path = self.base_dir.join(resource.filename());
        match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::error!(
                    target: "brazen::servo::resources",
                    path = %path.display(),
                    %error,
                    "failed to read servo resource"
                );
                Vec::new()
            }
        }
    }

    fn sandbox_access_files(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn sandbox_access_files_dirs(&self) -> Vec<PathBuf> {
        vec![self.base_dir.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn write_required_resources(dir: &Path) {
        for name in REQUIRED_RESOURCES {
            let path = dir.join(name);
            let _ = fs::write(path, "placeholder");
        }
    }

    #[test]
    fn resolves_config_path_first() {
        let dir = tempdir().unwrap();
        let resources_dir = dir.path().join("resources");
        fs::create_dir_all(&resources_dir).unwrap();
        write_required_resources(&resources_dir);

        let resolution = resolve_resource_dir(resources_dir.to_str(), Some("/tmp/servo")).unwrap();
        assert_eq!(resolution.source, ResourceDirSource::Config);
        assert_eq!(resolution.path, resources_dir);
    }

    #[test]
    fn resolves_env_var_when_config_missing() {
        let dir = tempdir().unwrap();
        let servo_root = dir.path().join("servo");
        let resources_dir = servo_root.join("resources");
        fs::create_dir_all(&resources_dir).unwrap();
        write_required_resources(&resources_dir);

        let resolution = resolve_resource_dir(None, servo_root.to_str()).unwrap();
        assert_eq!(resolution.source, ResourceDirSource::EnvVar);
        assert_eq!(resolution.path, resources_dir);
    }

    #[test]
    fn resolve_requires_existing_directory() {
        let error = resolve_resource_dir(Some("missing-path"), None).unwrap_err();
        assert!(error.contains("resources directory not found"));
    }

    #[test]
    fn resolves_vendor_resources_when_no_overrides() {
        let vendor_path = PathBuf::from("vendor").join("servo").join("resources");
        if !vendor_path.is_dir() {
            return;
        }
        if validate_resources(&vendor_path).is_err() {
            return;
        }
        let resolution = resolve_resource_dir(None, None).unwrap();
        assert_eq!(resolution.source, ResourceDirSource::Vendor);
        assert!(resolution.path.ends_with("vendor/servo/resources"));
    }
}

#[cfg(all(test, feature = "servo-upstream"))]
mod servo_tests {
    use super::*;
    use libservo::resources::ResourceReaderMethods;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resource_reader_reads_known_resource() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("badcert.html");
        fs::write(&path, "<html>bad cert</html>").unwrap();
        let reader = ServoResourceReader::new(dir.path().to_path_buf());
        let bytes = reader.read(libservo::resources::Resource::BadCertHTML);
        assert!(!bytes.is_empty());
    }
}
