use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
use zip::ZipWriter;
use crate::utils::file_utils;

use crate::utils::file_utils::file_to_string;

pub struct Zip {
    writer: ZipWriter<File>,
    finished: bool,
}

impl Zip {
    pub fn new(path: &Path) -> Self {
        match file_utils::create_file(path){
            Ok(zip_file) => {
                let writer = ZipWriter::new(zip_file);
                let finished = false;
                Self { writer, finished }
            }
            Err(error) => {
                let error = format!("Failed creating zip file: {} with error: {}", path.to_string_lossy(), error);
                log::error!("{}", error);
                panic!("{}", error)
            }
        }

    }

    pub fn add_file_to_zip(&mut self, file: &Path) -> Result<usize, String> {
        self.add_file_to_zip_with_limit(file, 0)
    }

    pub fn add_file_to_zip_with_limit(&mut self, file: &Path, limit: usize) -> Result<usize, String> {
        if self.finished {
            return Err("Attempt to write into zip after it's already finished!".to_string());
        }
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);
        let parent = file.parent().ok_or("Parent fail!")?;
        let internal_path = file
            .strip_prefix(parent)
            .map_err(|_| "Strip fail!")?
            .to_string_lossy();
        log::trace!("Archiving {}", internal_path);
        self.writer
            .start_file(internal_path, options)
            .map_err(|_| "Add file fail!")?;
        let file_text = file_to_string(file).map_err(|e| format!("File read fail!: {}", e))?;
        let len = file_text.len();
        let (file_text, remaining) = if limit > 0 && len > limit {
            (&file_text.as_bytes()[len - limit..], 0)
        } else {
            let remaining = if limit > 0 { limit - len } else { 0 };
            (file_text.as_bytes(), remaining)
        };
        self.writer
            .write_all(file_text)
            .map_err(|_| "Write file fail!")?;
        Ok(remaining)
    }

    pub fn add_dir_to_zip(&mut self, dir: &PathBuf) -> Result<(), String> {
        if self.finished {
            return Err("Attempt to write into zip after it's already finished!".to_string());
        }
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);
        let parent = dir.parent().ok_or("Parent fail!")?;
        let internal_dir = dir.strip_prefix(parent).map_err(|_| "Strip fail!")?;
        log::trace!(
            "Archiving {:?} into {:?} in the zip file",
            &dir,
            &internal_dir
        );
        self.writer
            .add_directory(internal_dir.to_string_lossy(), Default::default())
            .map_err(|_| "Directory add fail!")?;
        let paths = std::fs::read_dir(dir).map_err(|_| "Read dir fail!")?;
        for path in paths {
            let path = path.map_err(|_| "Path fail!")?.path();
            let internal_path = path.strip_prefix(dir).map_err(|_| "Strip fail!")?;
            log::trace!("Archiving {}", internal_path.to_string_lossy());
            self.writer
                .start_file(internal_dir.join(internal_path).to_string_lossy(), options)
                .map_err(|_| "Add file fail!")?;
            let buffer = file_to_string(&path.clone()).map_err(|_| "File read fail!")?;
            self.writer
                .write_all(buffer.as_bytes())
                .map_err(|_| "Write file fail!")?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Err("Finished called but we're already finished!".to_string());
        }
        self.writer.finish().map_err(|_| "Zip archive fail!")?;
        self.finished = true;
        Ok(())
    }
}
