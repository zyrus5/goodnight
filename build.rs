use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    // sqlx::migrate! embeds migrations in the executable. Watching the directory
    // ensures that adding a migration invalidates every release target instead of
    // silently reusing a binary that contains an older migration set.
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/package-lock.json");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");
    println!("cargo:rerun-if-changed=frontend/tsconfig.json");
    println!("cargo:rerun-if-changed=frontend/tsconfig.app.json");
    println!("cargo:rerun-if-changed=frontend/tsconfig.node.json");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let frontend_dir = manifest_dir.join("frontend");

    if env::var("GOODNIGHT_SKIP_FRONTEND_BUILD").as_deref() == Ok("1") {
        assert!(
            frontend_dir.join("dist/index.html").is_file(),
            "GOODNIGHT_SKIP_FRONTEND_BUILD=1 requires frontend/dist/index.html"
        );
        return;
    }

    if !frontend_dir.join("node_modules").is_dir() {
        run_npm(&frontend_dir, &["ci"], "install frontend dependencies");
    }

    run_npm(&frontend_dir, &["run", "build"], "build frontend assets");
}

fn run_npm(frontend_dir: &Path, args: &[&str], action: &str) {
    let status = Command::new("npm")
        .args(args)
        .current_dir(frontend_dir)
        .status()
        .unwrap_or_else(|error| panic!("failed to {action}: could not start npm: {error}"));

    assert!(
        status.success(),
        "failed to {action}: npm exited with {status}"
    );
}
