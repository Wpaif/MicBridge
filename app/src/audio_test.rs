//! Test utilities for process-based audio tests; no sound hardware required.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub dir: PathBuf,
    pub executable: PathBuf,
}
impl Fixture {
    pub fn new(script: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "micbridge-process-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let executable = dir.join("process");
        let source = format!(
            "#!/bin/sh\n{}\n",
            script.replace("@DIR@", dir.to_str().unwrap())
        );
        std::fs::write(&executable, source).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { dir, executable }
    }
    pub fn pid(&self) -> Option<u32> {
        std::fs::read_to_string(self.dir.join("pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
pub fn wait(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < end, "process condition timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub fn alive(pid: u32) -> bool {
    PathBuf::from("/proc").join(pid.to_string()).exists()
}
pub const SLEEP: &str = "echo $$ > '@DIR@/pid.tmp'; mv '@DIR@/pid.tmp' '@DIR@/pid'; exec sleep 30";
