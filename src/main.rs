use anyhow::{Context, Result};
use std::fs;

// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let command = &args[3];
    let command_args = &args[4..];

    let dir = std::env::temp_dir().join("sandbox");
    fs::create_dir_all(&dir)?;

    let executable = dir.join("executable");
    fs::copy(command, executable)?;

    std::os::unix::fs::chroot(dir)?;
    std::env::set_current_dir("/")?;
    fs::create_dir_all("/dev/null")?;

    let output = std::process::Command::new("./executable")
        .args(command_args)
        .output()
        .with_context(|| {
            format!(
                "Tried to run '{}' with arguments {:?}",
                command, command_args
            )
        })?;

    let std_out = std::str::from_utf8(&output.stdout)?;
    print!("{}", std_out);
    let std_err = std::str::from_utf8(&output.stderr)?;
    eprint!("{}", std_err);

    if output.status.success() {
        std::process::exit(output.status.code().unwrap_or(0));
    } else {
        std::process::exit(output.status.code().unwrap_or(1));
    }
}
