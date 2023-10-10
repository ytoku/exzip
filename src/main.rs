mod encoding;
mod interrupt;
mod tempfile_utils;
mod zip_ext;

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _, Result};
use clap::Parser;
use encoding_rs::Encoding;

use crate::encoding::get_encoding;
use crate::interrupt::{interrupted, register_ctrlc};
use crate::tempfile_utils::{tempdir_with_prefix_in, RelativePathFrom};
use crate::zip_ext::ZipFileExt;

const EXIT_ERROR: i32 = 1;
const EXIT_INTERRUPT: i32 = 130;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'O')]
    oenc: Option<String>,

    zipfiles: Vec<String>,
}

fn interruptable_copy<R, W>(reader: &mut R, writer: &mut W) -> Result<u64>
where
    R: io::Read + ?Sized,
    W: io::Write + ?Sized,
{
    let mut written_length = 0usize;
    let mut buf = [0; 128 * 1024];
    let mut eof = false;
    while !eof {
        let mut pos = 0usize;
        while pos < buf.len() {
            let length = reader.read(&mut buf[pos..])?;
            if length == 0usize {
                eof = true;
                break;
            }
            pos += length;
        }
        writer.write_all(&buf[..pos])?;
        written_length += pos;

        if interrupted() {
            bail!("Interrupted");
        }
    }
    writer.flush()?;
    Ok(written_length as u64)
}

fn sanitize_path(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(os_str) => {
                // Reject a component which contains NUL character
                #[cfg(windows)]
                compile_error!("wide character is not supported");
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    if os_str.as_bytes().iter().any(|&x| x == 0u8) {
                        return None;
                    }
                }
                result.push(os_str);
            }
            std::path::Component::ParentDir => {
                if result.as_os_str() == "" {
                    return None;
                }
                result.pop();
            }
            // Remove Prefix(C:), RootDir(/), CurDir(.)
            _ => {}
        }
    }
    Some(result)
}

fn is_ignored_file(path: &Path) -> bool {
    path.iter().any(|name| name == "__MACOSX")
}

fn unzip<R>(
    archive: &mut zip::ZipArchive<R>,
    inner_root: &Path,
    dst_root: &Path,
    encoding: &'static Encoding,
) -> Result<bool>
where
    R: io::Read + io::Seek,
{
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let unstripped_path =
            sanitize_path(&file.decoded_name_lossy(encoding)).context("Malformed zip file")?;
        let Ok(path) = unstripped_path.strip_prefix(inner_root) else {
            println!("Skip {}", unstripped_path.to_string_lossy());
            if !is_ignored_file(&unstripped_path) {
                bail!("Unexpected strip_prefix: {:?}", inner_root);
            }
            continue;
        };
        let dst_path = dst_root.join(path);

        println!("{}", unstripped_path.to_string_lossy());
        if file.is_dir() {
            fs::create_dir_all(dst_path)?;
        } else if file.is_file() {
            fs::create_dir_all(dst_path.parent().unwrap())?;
            let mut outfile = File::create(&dst_path)?;
            interruptable_copy(&mut file, &mut outfile)?;
        }

        if interrupted() {
            bail!("Interrupted");
        }
    }
    Ok(true)
}

fn get_inner_root<R>(
    archive: &mut zip::ZipArchive<R>,
    encoding: &'static Encoding,
) -> Option<PathBuf>
where
    R: io::Read + io::Seek,
{
    if archive.is_empty() {
        return Some(PathBuf::new());
    }

    let mut root: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i).unwrap();
        let mut path = sanitize_path(&file.decoded_name_lossy(encoding))?;
        if is_ignored_file(&path) {
            continue;
        }
        if !file.is_dir() {
            path.pop();
        }
        if let Some(root) = &root {
            if !path.starts_with(root) {
                return Some(PathBuf::new());
            }
        } else if let Some(name) = path.iter().next() {
            // The first found directory
            root = Some(PathBuf::from(name));
        } else {
            // There is a file in root
            return Some(PathBuf::new());
        }
    }
    root
}

fn extract(zipfile: &Path, target_path: &Path, args: &Args) -> Result<()> {
    // TODO: guessing encoding
    let encoding_name = args.oenc.as_deref().unwrap_or("shift_jis");

    let Some(encoding) = get_encoding(encoding_name) else {
        bail!("Unknown encoding {}", encoding_name);
    };

    println!("unzip {}", zipfile.display());

    let temp_dir = tempdir_with_prefix_in(zipfile.parent().unwrap(), "exzip-")?;
    let temp_dir_path = temp_dir.relative_path_from("./");

    let file = File::open(zipfile).unwrap();
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader).unwrap();

    let inner_root =
        get_inner_root(&mut archive, encoding).context("Failed to determine inner root")?;

    unzip(&mut archive, &inner_root, &temp_dir_path, encoding)?;

    println!(
        "rename {} -> {}",
        &temp_dir_path.display(),
        target_path.display()
    );

    if target_path.exists() {
        fs::remove_dir_all(target_path).expect("Failed to remove the old directory");
    }
    fs::rename(temp_dir.path(), target_path).expect("Failed to move the directory");

    Ok(())
}

fn main() {
    register_ctrlc();

    let args = Args::parse();

    for filename in &args.zipfiles {
        if !filename.ends_with(".zip") {
            eprintln!("Bad filename {}", filename);
            std::process::exit(EXIT_ERROR);
        }
        if !Path::new(&filename).exists() {
            eprintln!("Not found {}", filename);
            std::process::exit(EXIT_ERROR);
        }
    }

    for filename in &args.zipfiles {
        let target_path = Path::new(filename.strip_suffix(".zip").unwrap());

        let filepath = Path::new(&filename);

        let mut success = true;
        extract(filepath, target_path, &args).unwrap_or_else(|err| {
            eprintln!("Error: {:?}", err);
            success = false;
        });

        if interrupted() {
            std::process::exit(EXIT_INTERRUPT);
        }
        if !success {
            std::process::exit(EXIT_ERROR);
        }
    }
}
