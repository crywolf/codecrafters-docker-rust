use anyhow::{Context, Result};
use std::fs;

// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let command = &args[3];
    let command_args = &args[4..];

    isolate_filesystem(command)?;
    isolate_process();

    // Run the command
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

/// Filesystem isolation
fn isolate_filesystem(command: &str) -> Result<()> {
    let dir = tempfile::tempdir()?.into_path();

    let executable = dir.join("executable");
    fs::copy(command, executable).context("copying command executable")?;

    std::os::unix::fs::chroot(dir).context("calling chroot")?;
    std::env::set_current_dir("/")?;
    fs::create_dir_all("/dev/null")?;

    Ok(())
}

/// Process isolation
fn isolate_process() {
    // Isolates the newly created process from the host system's processes. It will get a PID of 1.
    // https://man7.org/linux/man-pages/man7/namespaces.7.html
    // The unshare(2) system call moves the calling process to a
    // new namespace.  If the flags argument of the call
    // specifies one or more of the CLONE_NEW* flags listed
    // above, then new namespaces are created for each flag, and
    // the calling process is made a member of those namespaces.
    unsafe { libc::unshare(libc::CLONE_NEWPID) };
}
