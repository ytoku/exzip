mod encoding;
mod interrupt;
mod tempfile_utils;
mod zip_ext;

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _, Result};
use cap_fs_ext::{DirExt, SystemTimeSpec};
use cap_primitives::time::SystemTime;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use clap::Parser;
use zip::ZipArchive;

use crate::encoding::{get_encoding, ZipEncoding};
use crate::interrupt::{interrupted, register_ctrlc};
use crate::tempfile_utils::{tempdir_with_prefix_in, TempDirExt};
use crate::zip_ext::ZipFileExt;

const EXIT_ERROR: i32 = 1;
const EXIT_INTERRUPT: i32 = 130;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'O')]
    oenc: Option<String>,

    zipfiles: Vec<PathBuf>,
}

// TODO: Readへのwrapperで再実装を検討。interrupted呼び出し回数が増えて遅くなる？
fn interruptable_copy<R, W>(reader: &mut R, writer: &mut W) -> Result<u64>
where
    R: io::Read + ?Sized,
    W: io::Write + ?Sized,
{
    let mut written_length = 0usize;
    let mut buf = [0u8; 128 * 1024];
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
        use std::path::Component;
        match component {
            Component::Normal(os_str) => {
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
            Component::ParentDir => {
                if result == Path::new("") {
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
    if path.iter().any(|name| name == "__MACOSX") {
        return true;
    }
    if let Some(filename) = path.file_name() {
        if ["Thumbs.db", ".DS_Store"]
            .iter()
            .any(|name| &filename == name)
        {
            return true;
        }
    }
    false
}

fn unzip<R>(
    archive: &mut ZipArchive<R>,
    inner_root: &Path,
    dst_root: &Dir,
    encoding: ZipEncoding,
) -> Result<()>
where
    R: io::Read + io::Seek,
{
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let unstripped_path =
            sanitize_path(&file.decoded_name_lossy(encoding)).context("Malformed zip file")?;
        let path = match unstripped_path.strip_prefix(inner_root) {
            Ok(path) if path == Path::new("") => Path::new("."),
            Ok(path) => path,
            _ => {
                println!("Skip {}", unstripped_path.to_string_lossy());
                if !is_ignored_file(&unstripped_path) {
                    bail!("Unexpected strip_prefix: {:?}", inner_root);
                }
                continue;
            }
        };

        if is_ignored_file(&unstripped_path) {
            println!("Skip {}", unstripped_path.to_string_lossy());
            continue;
        }

        println!("{}", unstripped_path.to_string_lossy());
        if file.is_dir() {
            dst_root.create_dir_all(path)?;
        } else if file.is_file() {
            dst_root.create_dir_all(path.parent().unwrap())?;
            let mut outfile = dst_root.create(path)?;
            interruptable_copy(&mut file, &mut outfile)?;
        }

        // Set last modified time
        // for DST overlap, select the earliest datetime of ambiguous one.
        // Some zip files contain invalid mtime such as 1980-00-00 00:00:00.
        // In such case, we do not set the mtime.
        if let Some(mtime_datetime) = file.last_modified_chrono().earliest(/* for DST overlap */) {
            let mtime = SystemTimeSpec::Absolute(SystemTime::from_std(mtime_datetime.into()));
            dst_root.set_mtime(path, mtime)?;
        }

        // We won't apply symlinks and permissions by design.

        if interrupted() {
            bail!("Interrupted");
        }
    }
    Ok(())
}

fn get_inner_root<R>(archive: &mut ZipArchive<R>, encoding: ZipEncoding) -> Result<PathBuf>
where
    R: io::Read + io::Seek,
{
    if archive.is_empty() {
        return Ok(PathBuf::new());
    }

    let mut root: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i)?;
        let mut path =
            sanitize_path(&file.decoded_name_lossy(encoding)).context("Malformed zip file")?;
        if is_ignored_file(&path) {
            continue;
        }
        if !file.is_dir() {
            path.pop();
        }
        if let Some(root) = &root {
            if !path.starts_with(root) {
                return Ok(PathBuf::new());
            }
        } else if let Some(name) = path.iter().next() {
            // The first found directory
            root = Some(PathBuf::from(name));
        } else {
            // There is a file in root
            return Ok(PathBuf::new());
        }
    }
    Ok(root.unwrap_or_default())
}

fn detect_filename_encoding<R>(archive: &mut ZipArchive<R>) -> Result<ZipEncoding>
where
    R: io::Read + io::Seek,
{
    for candidate_encoding in &[encoding_rs::UTF_8, encoding_rs::SHIFT_JIS] {
        let mut mismatch = false;
        for i in 0..archive.len() {
            let file = archive.by_index_raw(i)?;
            if file.is_utf8() {
                continue;
            }
            let (_cow, _encoding, malformed) = candidate_encoding.decode(file.name_raw());
            if malformed {
                mismatch = true;
                break;
            }
        }
        if !mismatch {
            return Ok(ZipEncoding::EncodingRs(candidate_encoding));
        }
    }
    Ok(ZipEncoding::Cp437)
}

fn extract_into(zipfile: &Path, target_path: &Path, args: &Args) -> Result<()> {
    let temp_dir_obj = tempdir_with_prefix_in(zipfile.parent().unwrap(), "exzip-")?;
    let temp_dir_path = temp_dir_obj.relative_path_from("./");
    let temp_dir = Dir::open_ambient_dir(temp_dir_obj.path(), ambient_authority())?;

    let file = File::open(zipfile)?;
    let reader = BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;

    let encoding = if let Some(encoding_name) = &args.oenc {
        get_encoding(encoding_name).unwrap()
    } else {
        detect_filename_encoding(&mut archive)?
    };

    let inner_root =
        get_inner_root(&mut archive, encoding).context("Failed to determine inner root")?;

    unzip(&mut archive, &inner_root, &temp_dir, encoding)?;

    println!(
        "rename {} -> {}",
        temp_dir_path.display(),
        target_path.display()
    );

    if target_path.exists() {
        fs::remove_dir_all(target_path).expect("Failed to remove the old directory");
    }
    fs::rename(temp_dir_obj.path(), target_path).expect("Failed to move the directory");

    Ok(())
}

fn extract(zipfile: &Path, args: &Args) -> Result<()> {
    println!("unzip {}", zipfile.display());

    let target_path = zipfile.with_extension("");

    if target_path.exists() {
        println!("Already exists: {}", target_path.display());
        let input = dialoguer::Confirm::new()
            .with_prompt("Replace?")
            .default(false)
            .interact()
            .map_err(|err| match err {
                dialoguer::Error::IO(ref inner) if inner.kind() == io::ErrorKind::Interrupted => {
                    anyhow::anyhow!("Interrupted")
                }
                _ => anyhow::Error::from(err),
            })?;
        if !input {
            return Ok(());
        }
    }

    extract_into(zipfile, &target_path, args)
}

fn main() {
    register_ctrlc();

    let args = Args::parse();

    if let Some(encoding_name) = &args.oenc {
        if get_encoding(encoding_name).is_none() {
            println!("Error: Unknown encoding {}", encoding_name);
            std::process::exit(EXIT_ERROR);
        }
    }

    for filepath in &args.zipfiles {
        if filepath.extension().is_none() {
            eprintln!("Bad filename {}", filepath.display());
            std::process::exit(EXIT_ERROR);
        }
        if !filepath.exists() {
            eprintln!("Not found {}", filepath.display());
            std::process::exit(EXIT_ERROR);
        }
        if !filepath.is_file() {
            eprintln!("Not a file {}", filepath.display());
            std::process::exit(EXIT_ERROR);
        }
    }

    for filepath in &args.zipfiles {
        let mut success = true;
        extract(filepath, &args).unwrap_or_else(|err| {
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
