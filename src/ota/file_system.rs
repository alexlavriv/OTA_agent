use std::path::Path;

pub struct FileSystem {
    pub write_function: fn(path: &Path, content: &str) -> Result<(), String>,
    pub read_function: fn(path: &Path) -> Result<String, String>,
    pub empty_folder: fn(path: &Path) -> Result<(), String>,
    pub remove_file: fn(path: &Path) -> Result<(), String>,
}
