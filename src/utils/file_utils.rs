use crypto::{digest::Digest, sha1::Sha1};
use log;
use serde_json;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
use std::ffi::OsStr;


pub(crate) fn generate_file_error(file_name: &Path, description: &str) -> String {
    if file_name.is_absolute() {
        format!("Cannot read {} {}", file_name.display(), description)
    } else {    // path is not absolute
        let current_dir = String::from(std::env::current_dir().unwrap().to_str().unwrap());
        format!(
            "Cannot read {}/{} {}",
            current_dir,
            file_name.display(),
            description
        )
    }
}


pub fn file_to_json(file_name: &Path) -> Result<serde_json::Value, String> {
    match file_to_string(file_name) {
        Ok(file_content) => string_to_json(&file_content),
        Err(error) => Err(error),
    }
}

pub fn create_dir_if_not_exists(dir_path: &Path) {
    if !dir_path.exists() {
        fs::create_dir_all(dir_path).unwrap();
    }
}

pub fn string_to_json(json_string: &str) -> Result<serde_json::Value, String> {
    match serde_json::from_str(json_string) {
        Ok(json) => Ok(json),
        Err(error) => Err(error.to_string()),
    }
}

pub fn mv_file(source: &Path, dest: &Path) -> Result<(), String> {
    if !source.exists() {
        return Err(format!(
            "copy_if_not_exists: {} doest not exist",
            source.to_str().unwrap()
        ));
    }
    if dest.exists() {
        if let Err(e) = fs::remove_file(dest) {
            return Err(format!(
                "Error removing {}: {:?}",
                dest.to_string_lossy(),
                e.kind()
            ));
        }
    }
    if let Err(e) = fs::copy(source, dest) {
        return Err(format!(
            "Error copying {} to {}: {:?}",
            source.to_string_lossy(),
            dest.to_string_lossy(),
            e.kind()
        ));
    }
    // ignore result
    rm_file(source).ok();
    Ok(())
}

pub fn rm_file(source: &Path) -> Result<(), String> {
    if !source.exists() {
        log::info!("{} does not exist", source.to_string_lossy());
        return Ok(());
    }

    match fs::remove_file(source) {
        Ok(_) => {
            log::info!("File {} removed successfully", source.to_string_lossy());
            Ok(())
        }
        Err(error) => {
            log::error!(
                "Failed removing {} with error {}",
                source.to_string_lossy(),
                error.to_string()
            );
            Err(error.to_string())
        }
    }
}

// Returns none if the copy succeed or the destination exist, otherwise error
pub fn copy_if_not_exists(source: &Path, dest: &Path) -> Option<String> {
    if !source.exists() {
        println!(
            "copy_if_not_exists: {} doest not exist",
            source.to_str().unwrap()
        );
        return Some(format!("{} does not exist", source.to_str().unwrap()));
    }
    if !dest.exists() {
        if !dest.parent().unwrap().exists() {
            println!("creating parent dir");
            fs::create_dir(dest.parent().unwrap()).unwrap();
        }
        println!("copying source to dest");
        fs::copy(source, dest).unwrap();
    }
    None
}

pub fn create_file(file_name: &Path) -> Result<File, String>{
    if file_name.exists(){
        Err(generate_file_error(file_name, "Cannot create the file, a file with the name already exists"))
    }else {
        Ok(File::create(file_name).map_err(|err|format!("Failed creating file: {} with error: {}", file_name.to_string_lossy(), err))?)
    }
}

pub fn string_to_file(file_name: &Path, content: &str) -> Result<(), String> {
    let res = File::create(file_name);
    match res {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Err(error) => Err(error.to_string()),
            Ok(()) => Ok(()),
        },
        Err(error) => Err(error.to_string()),
    }
}

pub fn verify_valid_file(file_name: &Path) -> Result<(), String> {
    if file_name.as_os_str().is_empty() { Err(String::from("The path is empty")) }
    else if !file_name.exists() { Err(generate_file_error(file_name, "does not exist")) }
    else if file_name.is_dir() { Err(generate_file_error(file_name, "is a directory, not a file")) }
    else {
        Ok(())
    }
}

pub fn file_to_bytes(file_name: &Path) -> Result<Vec<u8>, String> {
    verify_valid_file(file_name)?;
    let file = File::open(file_name).map_err(|err|
        generate_file_error(file_name, &err.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let result = reader.read_to_end(&mut buffer);
    match result {
        Ok(_content) => Ok(buffer),
        Err(e) => Err(e.to_string()),
    }
}

pub fn read_file_encoding(file_name: PathBuf) -> Result<&'static encoding_rs::Encoding, String> {
    verify_valid_file(&file_name)?;
    let mut file = File::open(file_name).map_err(|_| "Could not open file!").unwrap();
    let mut buf = [0u8; 2];
    if file.read_exact(&mut buf).is_err() { return Ok(encoding_rs::UTF_8); }
    if buf == [255, 254] { return Ok(encoding_rs::UTF_16LE); }
    Ok(encoding_rs::UTF_8)
}

pub fn file_to_string(file_name: &Path) -> Result<String, String> {
    verify_valid_file(file_name)?;
    let encoding = Some(read_file_encoding(file_name.to_owned())?);
    let file = File::open(file_name).map_err(|e| e.to_string())?;
    let mut content = String::new();
    let mut rdr = encoding_rs_io::DecodeReaderBytesBuilder::new()
        .bom_sniffing(true)
        .strip_bom(true)
        .encoding(encoding)
        .build(file);
    rdr.read_to_string(&mut content).map_err(|e| e.to_string())?;
    Ok(content)
}

pub fn clear_folder(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(_) => match fs::create_dir(path) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "failure in creating dir {}: {}",
                path.to_str().unwrap(),
                e
            )),
        },
        Err(e) => Err(format!(
            "failure in removing dir {}: {}",
            path.to_str().unwrap(),
            e
        )),
    }
}

// This function takes last *size* kb of file, stores in a separate file and returns its path.
pub fn get_file_tail(path: &Path, size: usize) -> Result<PathBuf, String> {
    let file_content = file_to_bytes(path)?;
    let content_output = if file_content.len() < size {
        file_content
    } else {
        file_content.as_slice()[file_content.len() - size..].to_vec()
    };
    let store_path = PathBuf::from(path);
    let store_path = match store_path.parent() {
        None => return Err(format!("The path {} doesn't have parent folder", store_path.display())),
        Some(parent_path) =>
            parent_path.join([path.file_name().unwrap_or(OsStr::new("default_name")).to_str().unwrap_or_default(), "_tail"].join(""))
    };
    let mut file =
        File::create(store_path.clone()).map_err(|err| generate_file_error(store_path.as_path(), &err.to_string()))?;
    file.write_all(&content_output).map_err(|err| generate_file_error(store_path.as_path(), &err.to_string()))?;
    Ok(store_path)
}

// This function gets a top (absolute) directory, a relative file, and a relative target file location
//  and if the file exists, it renames the file to the target location. In addition it will remove
//  the directory the file is in if it's empty after it's moved, so it does allow you to rename
//  the file to its old directory name, provided it's the only one in the directory
pub fn move_if_exists(
    user_common_path: &Path,
    old_file_path: &Path,
    new_file_path: &Path,
) -> Result<(), String> {
    let dirname = Path::new("move_if_exist_tmp_path");
    let temporary_path = user_common_path.join(dirname);
    let joined_old_path = user_common_path.join(Path::new(old_file_path));
    let joined_new_path = user_common_path.join(Path::new(new_file_path));
    if joined_old_path.exists() {
        let joined_old_dir = joined_old_path.as_path().parent().unwrap();
        fs::rename(&joined_old_path, &temporary_path).map_err(|e| e.to_string())?;
        // Removing dir if empty after we moved its last content out
        if joined_old_dir.is_dir() && joined_old_dir.read_dir().unwrap().next().is_none() {
            if let Err(error) = fs::remove_dir(joined_old_dir) {
                // Revert move if failed to remove
                if let Err(another_error) = fs::rename(&temporary_path, &joined_old_path) {
                    return Err(another_error.to_string()); // Too many errors, abort the everything
                }
                return Err(error.to_string()); // Report the fail to remove
            }
        }
        if let Err(error) = fs::rename(&temporary_path, joined_new_path) {
            return Err(error.to_string()); // Failed to move the contents
        }
    }
    Ok(())
}

pub fn get_path(user_common_path: &Path, file_path: &Path) -> PathBuf {
    user_common_path.join(Path::new(file_path))
}

fn get_sha1_checksum_by_chunks(path: &Path, chunk_size: usize) -> Result<String, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            return Err(format!(
                "Failed to open {} for checksum: {}",
                path.to_str().unwrap(),
                e
            ));
        }
    };

    let mut reader = BufReader::with_capacity(chunk_size, file);
    let mut sh = Box::new(Sha1::new());
    loop {
        let length = {
            let buffer = match reader.fill_buf() {
                Ok(buffer) => buffer,
                Err(e) => {
                    return Err(format!(
                        "Failed to read into buffer from {}: {}",
                        path.to_str().unwrap(),
                        e
                    ));
                }
            };
            sh.input(buffer);
            buffer.len()
        };
        if length == 0 {
            break;
        }
        reader.consume(length);
    }
    let out_str = (*sh).result_str();
    assert_eq!(out_str.len(), 40);
    sh.reset();
    Ok(out_str)
}

pub fn get_sha1_checksum(path: &Path) -> Result<String, String> {
    const CAP: usize = 1024 * 128;
    get_sha1_checksum_by_chunks(path, CAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn basic() {
        let res = file_to_string(Path::new(""));
        match res {
            Ok(_) => unreachable!(),
            Err(error) => assert!(error.starts_with("The path is empty")),
        }

        assert!(file_to_string(Path::new("/"))
            .unwrap_err()
            .ends_with("is a directory, not a file"));

        let file_name = Path::new("foo.txt");
        let res = file_to_string(file_name);

        match res {
            Ok(_) => unreachable!(),
            Err(error) => assert!(error.ends_with("does not exist")),
        }

        let mut file = File::create(file_name).unwrap();
        file.write_all(b"alex").unwrap();
        let res = file_to_string(file_name);
        match res {
            Ok(text) => assert_eq!(text, "alex"),
            Err(_) => assert_eq!(1, 1),
        }
        fs::remove_file(file_name).unwrap();
    }

    // Building a directory structure of /tmp/x/[dirname]/[file with same name as dirname]
    //                                               +---/[another file with different name]
    // test_move_if_exists should turn turn it them into /tmp/x/[file with same name as dirname]
    //                                                        +-[another file with different name]
    // Provided that you move the second file first so the directory is empty and can be replaced
    #[test]
    fn test_move_if_exists_same_name_success() {
        // SETUP
        let top = std::env::current_dir().unwrap();
        let dirname = top.join(Path::new("tmp_dir_move_if_exists_success"));
        if dirname.exists() {
            fs::remove_dir_all(&dirname).expect("Failed to remove old dir!");
        }
        fs::create_dir(&dirname).expect("Failed to create dir!");
        let dir_same_as_file = Path::new("dir_same_as_file");
        let just_a_file = Path::new("just_a_file");
        let subdirname = dirname.join(Path::new(&dir_same_as_file));
        fs::create_dir(&subdirname).expect("Failed to create subdir!");
        let data1 = "TEST_FILE1";
        let data2 = "TEST_FILE2";
        fs::write(subdirname.join(just_a_file), data1).expect("Couldn't write into file!");
        fs::write(subdirname.join(dir_same_as_file), data2).expect("Couldn't write into file!");
        assert_eq!(
            fs::read_to_string(subdirname.join(just_a_file)).expect("Couldn't read from file!"),
            data1
        );
        assert_eq!(
            fs::read_to_string(subdirname.join(dir_same_as_file))
                .expect("Couldn't read from file!"),
            data2
        );
        // TEST
        let just_a_file_path = subdirname.join(Path::new(&just_a_file));
        move_if_exists(&dirname, &just_a_file_path, just_a_file).expect("Move if exists error!");
        let dir_same_as_file_path = subdirname.join(Path::new(&dir_same_as_file));
        move_if_exists(&dirname, &dir_same_as_file_path, dir_same_as_file)
            .expect("Move if exists error!");
        assert_eq!(
            fs::read_to_string(dirname.join(just_a_file)).expect("Couldn't read from file!"),
            data1
        );
        assert_eq!(
            fs::read_to_string(dirname.join(dir_same_as_file)).expect("Couldn't read from file!"),
            data2
        );
        // CLEANUP
        fs::remove_dir_all(&dirname).expect("Failed to remove test dir!");
    }

    // Building a directory structure of /tmp/x/[dirname]/[file with same name as dirname]
    //                                               +---/[another file with different name]
    // test_move_if_exists should FAIL when you try to move the first file before clearing
    // the directory with the same name entirely
    #[test]
    fn test_move_if_exists_same_name_fail() {
        // SETUP
        let top = std::env::current_dir().unwrap();
        let dirname = top.join(Path::new("tmp_dir_move_if_exists_fail"));
        if dirname.exists() {
            fs::remove_dir_all(&dirname).expect("Failed to remove old dir!");
        }
        fs::create_dir(&dirname).expect("Failed to create dir!");
        let dir_same_as_file = Path::new("dir_same_as_file");
        let just_a_file = Path::new("just_a_file");
        let subdirname = dirname.join(Path::new(&dir_same_as_file));
        fs::create_dir(&subdirname).expect("Failed to create subdir!");
        let data1 = "TEST_FILE1";
        let data2 = "TEST_FILE2";
        fs::write(subdirname.join(just_a_file), data1).expect("Couldn't write into file!");
        fs::write(subdirname.join(dir_same_as_file), data2).expect("Couldn't write into file!");
        assert_eq!(
            fs::read_to_string(subdirname.join(just_a_file)).expect("Couldn't read from file!"),
            data1
        );
        assert_eq!(
            fs::read_to_string(subdirname.join(dir_same_as_file))
                .expect("Couldn't read from file!"),
            data2
        );
        // TEST
        let dir_same_as_file_path = subdirname.join(Path::new(&dir_same_as_file));
        let e = move_if_exists(&dirname, &dir_same_as_file_path, dir_same_as_file);
        e.unwrap_err(); // We expect to get an error here! Panic and fail the test if function returned Ok
        // CLEANUP
        fs::remove_dir_all(&dirname).expect("Failed to remove test dir!");
    }

    #[test]
    fn test_sha1_checksum_from_file() {
        let test_file = Path::new("sha1_checksum_from_file_test");
        if test_file.exists() {
            fs::remove_file(test_file).expect("Failed to remove old file");
        }
        let data = "The quick brown fox jumps over the lazy dog";
        fs::write(test_file, data).expect("Couldn't write into file!");
        let expected = "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12".to_string();
        let mut checksum = get_sha1_checksum_by_chunks(test_file, 1);
        assert_eq!(checksum.unwrap(), expected);
        checksum = get_sha1_checksum_by_chunks(test_file, 10);
        assert_eq!(checksum.unwrap(), expected);
        checksum = get_sha1_checksum(test_file);
        assert_eq!(checksum.unwrap(), expected);
        fs::remove_file(test_file).expect("Failed to remove temp file!");
    }

    #[test]
    fn test_file_tail() {
        let test_file = Path::new("file.log");
        if test_file.exists() {
            fs::remove_file(test_file).expect("Failed to remove old file");
        }
        let data = "The quick brown fox jumps over the lazy dog";
        fs::write(test_file, data).expect("Couldn't write into file!");

        let file_tail_path = get_file_tail(test_file, 5).expect("failed creating file tail");
        assert!(file_tail_path.exists());
        let file_content = file_to_string(&file_tail_path).expect("failed reading the file");
        assert_eq!(file_content.len(), 5);
        assert_eq!(file_content, "y dog");

        fs::remove_file(test_file).expect("Failed to remove temp file!");
        fs::remove_file(file_tail_path).expect("Failed to remove temp file!");
    }

    #[test]
    #[ignore]
    #[cfg(windows)]
    fn test_file_with_encoding() {
        let file_name= PathBuf::from("C:\\Users\\yurikarasik\\AppData\\Local\\Temp\\PHANTOM_MSI_INSTALLER\\install_log_oden_test.txt");
        //let file_name= PathBuf::from("C:\\Users\\yurikarasik\\AppData\\Local\\Temp\\PHANTOM_MSI_INSTALLER\\uninstall_log_test_component.txt");
        //let file_name= PathBuf::from("C:\\Program Files\\phantom_agent\\log\\phantom_agent.log");

        verify_valid_file(&file_name).unwrap();
        let start = std::time::Instant::now();
        let result = fs::read_to_string(&file_name);
        let duration = start.elapsed();
        println!("Time elapsed in fs::read_to_string is: {:?}", duration);
        match result {
            Ok(_) => {
                let start = std::time::Instant::now();
                let mut content = String::new();
                let file = File::open(file_name).map_err(|_| "Could not open file!").unwrap();
                let mut rdr = encoding_rs_io::DecodeReaderBytesBuilder::new()
                    .bom_sniffing(true)
                    .strip_bom(true)
                    .encoding(Some(encoding_rs::UTF_8))
                    .build(file);
                let duration = start.elapsed();
                println!("Time elapsed in making reader is: {:?}", duration);
                rdr.read_to_string(&mut content).map_err(|e| e.to_string()).unwrap();
                let duration = start.elapsed();
                println!("Time elapsed in reading utf-8 file is: {:?}", duration);
                println!("UTF8 CONTENT: {}", &content[0..100]);
            },
            Err(e) => {
                if e.to_string().contains("stream did not contain valid UTF-8")
                {   // Assuming UTF_16LE encoding and converting to UTF8
                    let start = std::time::Instant::now();
                    let mut content = String::new();
                    {
                        let mut file = File::open(file_name.clone()).map_err(|_| "Could not open file!").unwrap();
                        /*let mut rdr = encoding_rs_io::DecodeReaderBytesBuilder::new()
                        .encoding(Some(encoding_rs::UTF_16LE))
                        .build(file);*/

                        let mut buf = [0u8; 2];
                        file.read_exact(&mut buf).unwrap();
                        println!("BUF IS [{:?}]", buf);
                        if buf == [255, 254] {
                            println!("GOT THE BOM!");
                        }
                    }
                    let file = File::open(file_name).map_err(|_| "Could not open file!").unwrap();
                    let mut rdr = encoding_rs_io::DecodeReaderBytesBuilder::new()
                        .bom_sniffing(true)
                        .strip_bom(true)
                        .encoding(Some(encoding_rs::UTF_16LE))
                        .build(file);
                    let duration = start.elapsed();
                    println!("Time elapsed in making reader is: {:?}", duration);
                    //let got: Vec<u8> = rdr.bytes().map(|res| res.unwrap()).collect();
                    rdr.read_to_string(&mut content).map_err(|e| e.to_string()).unwrap();
                    let duration = start.elapsed();
                    println!("Time elapsed in reading 16le file is: {:?}", duration);
                    println!("16LE CONTENT: {}", &content[0..100]);
                }
            }
        }
    }
}
