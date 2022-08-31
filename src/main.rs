mod smart_tempdir;

use clap::Parser;
use smart_tempdir::smart_tempdir_in;
use std::fs;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const EXIT_ERROR: i32 = 1;
const EXIT_INTERRUPT: i32 = 130;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short = 'O', value_parser)]
    oenc: Option<String>,

    #[clap(short = 'I', value_parser)]
    ienc: Option<String>,

    #[clap(value_parser)]
    zipfiles: Vec<String>,
}

fn in_one_directory(zipfile: &Path) -> bool {
    let file = File::open(zipfile).unwrap();
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader).unwrap();

    let mut prev_dirname: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i).unwrap();
        let fname = file.name_raw();

        let parts: Vec<_> = fname.split(|c| *c == b'/').collect();
        if parts.len() < 2 {
            return false;
        }
        let cur_dirname = parts[0];

        match &prev_dirname {
            None => {
                prev_dirname = Some(Vec::from(cur_dirname));
            }
            Some(prev) => {
                if prev != cur_dirname {
                    return false;
                }
            }
        }
    }
    prev_dirname.is_some()
}

fn run_command(command: &mut Command) -> io::Result<()> {
    let exit_status = command.spawn()?.wait()?;
    if !exit_status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "Command failed"));
    }
    Ok(())
}

fn actual_inner_path(outer_path: &Path) -> Option<PathBuf> {
    let inner_entries: Vec<io::Result<fs::DirEntry>> = fs::read_dir(&outer_path).unwrap().collect();
    if inner_entries.len() != 1 {
        return None;
    }
    let inner_dirname = inner_entries[0].as_ref().ok()?.file_name();
    let inner_path = outer_path.join(&inner_dirname);
    Some(inner_path)
}

fn extract_one_directory(zipfile: &Path, target_path: &Path, options: Vec<&str>) -> io::Result<()> {
    let temp_dir = smart_tempdir_in(zipfile.parent().unwrap(), "exzip-")?;

    run_command(
        Command::new("unzip")
            .args(options)
            .arg("-d")
            .arg(temp_dir.path())
            .arg(zipfile),
    )?;

    let inner_path =
        actual_inner_path(temp_dir.path()).expect("Failed to determine actual inner path");

    fs::set_permissions(
        &inner_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("Failed to set permissions");

    // zipfile:                       dir/target.zip
    // base_path:                     dir
    // base_path_canonical:  /path/to/dir
    // inner_path_canonical: /path/to/dir/exzip-XXXXXX/a
    // relative_inner_path:           dir/exzip-XXXXXX/a
    let relative_inner_path = {
        let base_path = zipfile.parent().unwrap();
        let inner_path_canonical = inner_path.canonicalize().unwrap();
        let base_path_canonical = base_path.canonicalize().unwrap();
        base_path.join(
            inner_path_canonical
                .strip_prefix(base_path_canonical)
                .unwrap(),
        )
    };
    println!(
        "rename {} -> {}",
        relative_inner_path.display(),
        target_path.display()
    );

    if target_path.exists() {
        fs::remove_dir_all(target_path).expect("Failed to remove the old directory");
    }
    fs::rename(inner_path, target_path).expect("Failed to move the directory");

    temp_dir.close()?;
    Ok(())
}

fn extract_into_target_path(
    zipfile: &Path,
    target_path: &Path,
    options: Vec<&str>,
) -> io::Result<()> {
    let temp_dir = smart_tempdir_in(zipfile.parent().unwrap(), "exzip-")?;
    run_command(
        Command::new("unzip")
            .args(options)
            .arg("-d")
            .arg(temp_dir.path())
            .arg(zipfile),
    )?;

    // zipfile:                dir/target.zip
    // base_path:              dir
    // relative_temp_dir_path: dir/exzip-XXXXXX
    let relative_temp_dir_path = {
        let base_path = zipfile.parent().unwrap();
        base_path.join(temp_dir.path().file_name().unwrap())
    };
    println!(
        "rename {} -> {}",
        relative_temp_dir_path.display(),
        target_path.display()
    );

    if target_path.exists() {
        fs::remove_dir_all(target_path).expect("Failed to remove the old directory");
    }
    fs::rename(temp_dir.path(), target_path).expect("Failed to move the directory");

    Ok(())
}

fn extract(zipfile: &Path, target_path: &Path, one_directory: bool, args: &Args) -> io::Result<()> {
    let mut options: Vec<&str> = vec![];
    if let Some(ienc) = &args.ienc {
        options.push("-I");
        options.push(ienc);
    };
    if let Some(oenc) = &args.oenc {
        options.push("-O");
        options.push(oenc);
    };

    println!("unzip {} {}", options.join(" "), zipfile.display());

    if one_directory {
        extract_one_directory(zipfile, target_path, options)?;
    } else {
        extract_into_target_path(zipfile, target_path, options)?;
    }

    Ok(())
}

fn main() {
    let interrupted = Arc::new(AtomicBool::new(false));
    {
        let interrupted = interrupted.clone();
        ctrlc::set_handler(move || {
            interrupted.store(true, Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");
    }

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
        let one_directory = in_one_directory(filepath);

        let mut success = true;
        extract(filepath, target_path, one_directory, &args).unwrap_or_else(|err| {
            eprintln!("Error: {:?}", err);
            success = false;
        });

        if interrupted.load(Ordering::SeqCst) {
            std::process::exit(EXIT_INTERRUPT);
        }
        if !success {
            std::process::exit(EXIT_ERROR);
        }
    }
}
