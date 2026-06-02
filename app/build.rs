use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn main() {
    prepare_frontend_dist();

    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=frontend");
    println!("cargo:rerun-if-changed=icons/32x32.png");
    println!("cargo:rerun-if-changed=icons/128x128.png");
    println!("cargo:rerun-if-changed=icons/128x128@2x.png");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.icns");
    tauri_build::build()
}

fn prepare_frontend_dist() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let frontend_dir = manifest_dir.join("frontend");
    let dist_dir = manifest_dir.join("frontend-dist");

    if let Err(err) = rebuild_frontend_dist(&frontend_dir, &dist_dir) {
        panic!(
            "failed to prepare frontend dist from {} to {}: {err}",
            frontend_dir.display(),
            dist_dir.display()
        );
    }
}

fn rebuild_frontend_dist(frontend_dir: &Path, dist_dir: &Path) -> io::Result<()> {
    if !frontend_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("frontend directory not found: {}", frontend_dir.display()),
        ));
    }

    remove_if_exists(dist_dir)?;
    fs::create_dir_all(dist_dir)?;

    for entry in fs::read_dir(frontend_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
            fs::copy(&path, dist_dir.join(entry.file_name()))?;
        }
    }

    for dir in ["assets", "css", "js"] {
        copy_dir_recursive(&frontend_dir.join(dir), &dist_dir.join(dir))?;
    }
    copy_dir_recursive(
        &frontend_dir.join("__fixtures__").join("pets"),
        &dist_dir.join("__fixtures__").join("pets"),
    )?;

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "frontend runtime asset directory not found: {}",
                src.display()
            ),
        ));
    }

    fs::create_dir_all(dest)?;
    let mut entries = fs::read_dir(src)?.collect::<Result<Vec<_>, io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, dest_path)?;
        }
    }

    Ok(())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}
