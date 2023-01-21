use bytes::Bytes;
use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::path::{Component, PathBuf};
use zip::result::ZipResult;
use zip::{CompressionMethod, ZipWriter};

use zip::write::FileOptions;

#[derive(Debug, Clone)]
pub struct ZipStruct {
    pub is_dir: bool,
    pub target_dir: String,
    pub target_name: String,
    pub path: PathBuf,
}

pub fn zip_directory<W: Write + io::Seek>(file: W, directory: &Path) -> io::Result<()> {
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
    zip_directory_with_options(file, directory, options)
}

pub fn zip_directory_with_options<W: Write + io::Seek>(
    file: W,
    directory: &Path,
    options: FileOptions,
) -> io::Result<()> {
    let mut zip_writer = ZipWriter::new(file);
    let mut paths_queue: Vec<PathBuf> = vec![];
    paths_queue.push(directory.to_path_buf());

    let mut buffer = Vec::new();

    while let Some(next) = paths_queue.pop() {
        let directory_entry_iterator = std::fs::read_dir(next)?;

        for entry in directory_entry_iterator {
            let entry_path = entry?.path();
            let entry_metadata = std::fs::metadata(entry_path.clone())?;
            if entry_metadata.is_file() {
                let mut f = File::open(&entry_path)?;
                f.read_to_end(&mut buffer)?;
                let relative_path = make_relative_path(directory, &entry_path);
                #[allow(deprecated)]
                zip_writer.start_file_from_path(&relative_path, options)?;
                zip_writer.write_all(buffer.as_ref())?;
                buffer.clear();
            } else if entry_metadata.is_dir() {
                let relative_path = make_relative_path(directory, &entry_path);
                #[allow(deprecated)]
                zip_writer.add_directory_from_path(&relative_path, options)?;
                paths_queue.push(entry_path.clone());
            }
        }
    }

    zip_writer.finish()?;
    Ok(())
}

//state: AppState, zip_name: &str
pub fn create_zip_package(zip_structs: &[ZipStruct], zip_file: &mut File) -> io::Result<()> {
    let mut zip_writer = ZipWriter::new(zip_file);
    let target_dirs: HashSet<String> = zip_structs.iter().map(|x| &x.target_dir).cloned().collect();
    for dir in target_dirs.iter() {
        zip_writer
            .add_directory(dir, FileOptions::default())
            .unwrap();
    }
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o755);

    let mut buffer = Vec::new();
    for zip_struct in zip_structs.iter() {
        if !zip_struct.is_dir {
            let new_file_path = format!("{}/{}", &zip_struct.target_dir, &zip_struct.target_name);
            zip_writer.start_file(new_file_path, options).unwrap();
            let mut f = File::open(&zip_struct.path).unwrap();

            f.read_to_end(&mut buffer).unwrap();
            zip_writer.write_all(&buffer).unwrap();
            buffer.clear();
        } else {
            let directory = zip_struct.path.clone();
            let mut paths_queue: Vec<PathBuf> = vec![];
            paths_queue.push(directory.clone());

            while let Some(next) = paths_queue.pop() {
                let directory_entry_iterator = std::fs::read_dir(next)?;

                for entry in directory_entry_iterator {
                    let entry_path = entry?.path();

                    let entry_metadata = std::fs::metadata(entry_path.clone())?;
                    if entry_metadata.is_file() {
                        let mut f = File::open(&entry_path)?;
                        f.read_to_end(&mut buffer)?;
                        let relative_path = make_relative_path_with_base(
                            &zip_struct.target_dir,
                            &directory,
                            &entry_path,
                        );
                        #[allow(deprecated)]
                        zip_writer.start_file_from_path(&relative_path, options)?;
                        zip_writer.write_all(buffer.as_ref())?;
                        buffer.clear();
                    } else if entry_metadata.is_dir() {
                        let relative_path = make_relative_path_with_base(
                            &zip_struct.target_dir,
                            &directory,
                            &entry_path,
                        );
                        #[allow(deprecated)]
                        zip_writer.add_directory_from_path(&relative_path, options)?;
                        paths_queue.push(entry_path.clone());
                    }
                }
            }
        }
    }

    zip_writer.finish()?;
    Ok(())
}

pub(crate) fn make_relative_path_with_base(base: &str, root: &Path, current: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    result.push(base);
    let root_components = root.components().collect::<Vec<Component>>();
    let current_components = current.components().collect::<Vec<_>>();

    for i in 0..current_components.len() {
        let current_path_component: Component = current_components[i];
        if i < root_components.len() {
            let other: Component = root_components[i];
            if other != current_path_component {
                break;
            }
        } else {
            result.push(current_path_component);
        }
    }
    result
}

/// Returns a relative path from one path to another.
pub(crate) fn make_relative_path(root: &Path, current: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let root_components = root.components().collect::<Vec<Component>>();
    let current_components = current.components().collect::<Vec<_>>();
    for i in 0..current_components.len() {
        let current_path_component: Component = current_components[i];
        if i < root_components.len() {
            let other: Component = root_components[i];
            if other != current_path_component {
                break;
            }
        } else {
            result.push(current_path_component)
        }
    }
    result
}

/// Extracts a ZIP file to the given directory.
pub fn zip_extract_from_file(archive_file: &PathBuf, target_dir: &PathBuf) -> ZipResult<()> {
    let file = File::open(archive_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(target_dir)
}

/// Extracts a ZIP file from memory to the given directory.
pub fn zip_extract_from_memory(archive_file: &Bytes, target_dir: &PathBuf) -> ZipResult<()> {
    let reader = Cursor::new(archive_file);
    let mut archive = zip::ZipArchive::new(reader)?;
    archive.extract(target_dir)
}

#[cfg(test)]
mod tests {

    use super::{zip_directory, zip_extract_from_memory};

    #[test]
    fn test_zip_file_size_is_smaller() {
        let zip_file = include_bytes!("../../../../testing/unittests/test_zip.zip");
        let temp_dir = tempfile::TempDir::new()
            .expect("Could not create tmp directory")
            .into_path();
        let zip_bytes = bytes::Bytes::from_static(zip_file);
        zip_extract_from_memory(&zip_bytes, &temp_dir).expect("Could not extract archive");
        let dir_size = fs_extra::dir::get_size(&temp_dir).expect("Could not get size of directory");

        let mut tmp_file = tempfile::tempfile().expect("Could not create tempfile");
        zip_directory(&mut tmp_file, &temp_dir).expect("Could not zip file");
        let zipped_archive_size = tmp_file
            .metadata()
            .expect("Could not read tmp file metadata")
            .len();
        assert!(zipped_archive_size < dir_size)
    }
}
