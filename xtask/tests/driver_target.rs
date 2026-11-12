//! The xtask driver must not be rebuilt by the workspace test command on Windows.

#[cfg(windows)]
#[test]
fn xtask_alias_keeps_the_driver_out_of_the_workspace_target() {
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TargetDir(PathBuf);

    impl Drop for TargetDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock").as_nanos();
    let target = TargetDir(std::env::temp_dir().join(format!("cadcraft-xtask-alias-{}-{nonce}", std::process::id())));
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root").to_path_buf();
    for directory in [&workspace, &PathBuf::from(env!("CARGO_MANIFEST_DIR"))] {
        let output = Command::new(env!("CARGO"))
            .args(["xtask", "help"])
            .current_dir(directory)
            .env("CARGO_TARGET_DIR", &target.0)
            .env("CARGO_BUILD_JOBS", "2")
            .output()
            .expect("run cargo xtask help");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    // Cargo adds a target-triple directory when CARGO_BUILD_TARGET is configured.
    let mut target_roots = vec![target.0.clone()];
    target_roots.extend(
        std::fs::read_dir(&target.0).expect("read target directory").filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.is_dir()),
    );
    assert!(
        target_roots.iter().any(|path| path.join("xtask/xtask.exe").exists()),
        "the driver must honor CARGO_TARGET_DIR even from a nested directory"
    );
    assert!(
        target_roots.iter().all(|path| !path.join("debug/xtask.exe").exists()),
        "the xtask driver was built in the workspace test target, where nextest would try to replace the running executable"
    );
}
