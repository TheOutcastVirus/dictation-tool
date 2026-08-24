use std::path::PathBuf;
use std::process::Command;

const UNIT_NAME: &str = "dictation-tool.service";

/// Environment the login copy has to be started with to match this one.
///
/// ROCm selects its kernels from the gfx arch the runtime reports, and parts
/// whose arch ships no rocBLAS kernels (gfx1152, for one) only work when
/// `HSA_OVERRIDE_GFX_VERSION` points at a compatible sibling. That lives in
/// the launching shell, which systemd does not inherit, so a unit written
/// without it starts a daemon that dies at the first GEMM. Whatever the
/// working process was given is recorded here rather than hardcoded, so a
/// machine that needs no override gets no `Environment=` line at all.
fn inherited_env() -> String {
    ["HSA_OVERRIDE_GFX_VERSION"]
        .iter()
        .filter_map(|key| {
            let value = std::env::var(key).ok()?;
            Some(format!("Environment={key}={value}\n"))
        })
        .collect()
}

fn unit_template(repo_root: &std::path::Path, binary: &std::path::Path) -> String {
    format!(
        "[Unit]\n\
         Description=Dictation tool (single-binary Rust engine + GUI)\n\
         After=graphical-session.target\n\
         PartOf=graphical-session.target\n\
         \n\
         [Service]\n\
         {}\
         ExecStart={}\n\
         WorkingDirectory={}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
        inherited_env(),
        binary.display(),
        repo_root.display(),
    )
}

fn unit_path() -> PathBuf {
    dirs::config_dir()
        .expect("no config dir")
        .join("systemd/user")
        .join(UNIT_NAME)
}

/// Writes the unit file if missing or stale, and reloads the systemd user
/// manager. Called once at startup so setup is "build once, run once"
/// instead of a manual `cp` step.
pub fn ensure_unit_installed(repo_root: &std::path::Path, binary: &std::path::Path) {
    let desired = unit_template(repo_root, binary);
    let path = unit_path();

    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == desired {
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&path, desired).is_ok() {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
}

pub fn is_enabled() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", UNIT_NAME])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Enables/disables the unit for the next login. Deliberately *not*
/// `--now`: this binary is single-instance, so `enable --now` from inside a
/// manually launched copy would just spawn a second copy that immediately
/// exits, and `disable --now` from inside the systemd-launched copy would
/// kill the process the user is clicking in.
pub fn set_enabled(enabled: bool) -> bool {
    let action = if enabled { "enable" } else { "disable" };
    Command::new("systemctl")
        .args(["--user", action, UNIT_NAME])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
