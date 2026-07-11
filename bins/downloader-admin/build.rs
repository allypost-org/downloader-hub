use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

const FRONTEND_DIR: &str = "frontend";
const DIST_INDEX: &str = "frontend/dist/index.html";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let frontend_dir = manifest_dir.join(FRONTEND_DIR);
    let dist_index = manifest_dir.join(DIST_INDEX);

    for watched in watched_files(&frontend_dir) {
        println!("cargo:rerun-if-changed={}", watched.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    if env::var_os("SKIP_FRONTEND_BUILD").is_some() {
        println!("cargo:warning=[admin-frontend] SKIP_FRONTEND_BUILD set; skipping SPA build");
        return;
    }

    if !is_stale(&frontend_dir, &dist_index) {
        return;
    }

    if dist_index.try_exists().unwrap_or(false) {
        println!("cargo:warning=[admin-frontend] SPA sources changed; rebuilding");
    } else {
        println!("cargo:warning=[admin-frontend] dist/index.html missing; building SPA");
    }

    let bun = match which_bun() {
        Some(b) => b,
        None => {
            println!(
                "cargo:warning=[admin-frontend] bun not found on PATH; SPA not rebuilt. Run `bun \
                 run build` in bins/downloader-admin/frontend manually."
            );
            return;
        }
    };

    if !ensure_node_modules(&frontend_dir, &bun) {
        println!(
            "cargo:warning=[admin-frontend] `bun install` failed; SPA not rebuilt. Run `bun \
             install` in bins/downloader-admin/frontend manually."
        );
        return;
    }

    let status = Command::new(&bun)
        .arg("run")
        .arg("build")
        .current_dir(&frontend_dir)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=[admin-frontend] SPA built");
        }
        Ok(s) => {
            panic!("[admin-frontend] `bun run build` failed with status {s}");
        }
        Err(e) => {
            panic!("[admin-frontend] failed to run `bun run build`: {e}");
        }
    }
}

fn watched_files(frontend_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        frontend_dir.join("index.html"),
        frontend_dir.join("package.json"),
        frontend_dir.join("bun.lock"),
        frontend_dir.join("vite.config.ts"),
        frontend_dir.join("tsconfig.json"),
    ];
    if let Ok(entries) = std::fs::read_dir(frontend_dir.join("src")) {
        for entry in entries.flatten() {
            collect_recursive(&entry.path(), &mut out);
        }
    }
    out
}

fn collect_recursive(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_recursive(&entry.path(), out);
            }
        }
    } else {
        out.push(path.to_path_buf());
    }
}

fn is_stale(frontend_dir: &Path, dist_index: &Path) -> bool {
    let dist_mtime = match file_mtime(dist_index) {
        Some(t) => t,
        None => return true,
    };

    for watched in watched_files(frontend_dir) {
        if let Some(t) = file_mtime(&watched)
            && t > dist_mtime
        {
            return true;
        }
    }
    false
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn which_bun() -> Option<PathBuf> {
    if let Some(b) = env::var_os("BUN") {
        let p = PathBuf::from(b);
        if p.is_file() {
            return Some(p);
        }
    }
    let candidates = ["bun", "./bun"];
    for c in candidates {
        if Command::new(c).arg("--version").output().is_ok() {
            return Some(PathBuf::from(c));
        }
    }
    None
}

fn ensure_node_modules(frontend_dir: &Path, bun: &Path) -> bool {
    if frontend_dir.join("node_modules").is_dir() {
        return true;
    }
    println!("cargo:warning=[admin-frontend] node_modules missing; running `bun install`");
    Command::new(bun)
        .arg("install")
        .current_dir(frontend_dir)
        .status()
        .is_ok_and(|s| s.success())
}
