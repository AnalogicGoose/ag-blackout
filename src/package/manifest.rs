#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

impl PackageManifest {
    pub fn new(name: &str, version: &str, description: &str, dependencies: &[&str]) -> Self {
        PackageManifest {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            dependencies: dependencies.iter().map(|d| d.to_string()).collect(),
        }
    }
}
