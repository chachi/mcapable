fn main() {
    let mut out_dir = None;
    if std::env::var_os("MCAPABLE_BAZEL").is_some()
        && let Ok(value) = std::env::var("OUT_DIR")
    {
        unsafe {
            std::env::set_var("CARGO_TARGET_DIR", &value);
        }
        out_dir = Some(value);
    }

    cxx_build::bridge("src/lib.rs")
        .std("c++17")
        .compile("mcapable-cpp");
    println!("cargo:rerun-if-changed=src/lib.rs");

    if let Some(out_dir) = out_dir {
        normalize_symlinks(std::path::Path::new(&out_dir).join("cxxbridge"))
            .expect("normalize cxxbridge symlinks");
    }
}

fn normalize_symlinks(root: std::path::PathBuf) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            normalize_symlinks(path)?;
            continue;
        }
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let target = std::fs::read_link(&path)?;
        let target_path = if target.is_absolute() {
            target
        } else {
            path.parent().unwrap().join(target)
        };
        if target_path.is_dir() {
            std::fs::remove_file(&path)?;
            continue;
        }
        if target_path.is_file() {
            std::fs::remove_file(&path)?;
            std::fs::copy(target_path, &path)?;
        }
    }
    Ok(())
}
