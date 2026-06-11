use std::io::{self, Read};

use std::{
    process::{Command as ProcessCommand, Output as ProcessOutput, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub(crate) fn percentile_us(sorted_values: &[u64], percentile: f64) -> u64 {
    let last_index = sorted_values.len().saturating_sub(1);
    let index = (last_index as f64 * percentile).ceil() as usize;

    sorted_values[index.min(last_index)]
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn peak_rss_bytes() -> u64 {
    getrusage_maxrss().unwrap_or_default()
}

#[cfg(target_os = "freebsd")]
pub(crate) fn peak_rss_bytes() -> u64 {
    getrusage_maxrss().unwrap_or_default()
}

#[cfg(unix)]
pub(crate) fn getrusage_maxrss() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let usage = unsafe { usage.assume_init() };
    (usage.ru_maxrss >= 0).then_some(usage.ru_maxrss as u64)
}

#[cfg(not(unix))]
pub(crate) fn peak_rss_bytes() -> u64 {
    0
}

pub(crate) fn baseline_smoke_error(output: &ProcessOutput, timed_out: bool) -> Option<String> {
    if timed_out {
        Some("baseline smoke timed out".to_string())
    } else if !output.status.success() {
        Some(format!(
            "baseline smoke exited with status {:?}",
            output.status.code()
        ))
    } else {
        None
    }
}

pub(crate) fn baseline_process_error_kind(
    output: &ProcessOutput,
    timed_out: bool,
) -> Option<&'static str> {
    if timed_out {
        Some("timeout")
    } else if output.status.code() == Some(127)
        || (!output.status.success() && process_stderr_indicates_missing_dependency(&output.stderr))
    {
        Some("missing_dependency")
    } else if !output.status.success() {
        Some("execution_failed")
    } else {
        None
    }
}

pub(crate) fn process_stderr_indicates_missing_dependency(stderr: &[u8]) -> bool {
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    stderr.contains("command not found")
        || stderr.contains("no such file or directory")
        || stderr.contains("error opening data file")
        || stderr.contains("tessdata_prefix")
        || stderr.contains("failed loading language")
        || stderr.contains("couldn't load any languages")
}

pub(crate) fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

pub(crate) struct TimedProcessOutput {
    pub(crate) output: ProcessOutput,
    pub(crate) timed_out: bool,
    pub(crate) wall_us: u128,
}

pub(crate) fn command_output_with_timeout(
    mut command: ProcessCommand,
    timeout: Duration,
) -> io::Result<TimedProcessOutput> {
    let start = Instant::now();
    configure_timeout_command(&mut command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr was not piped"))?;
    let stdout_handle = thread::spawn(move || read_process_stream(stdout));
    let stderr_handle = thread::spawn(move || read_process_stream(stderr));
    let mut timed_out = false;

    let status = loop {
        if start.elapsed() >= timeout {
            timed_out = true;
            kill_timed_out_child(&mut child);
            break child.wait()?;
        }

        if let Some(status) = child.try_wait()? {
            break status;
        }

        thread::sleep(Duration::from_millis(5));
    };

    let stdout = join_process_reader(stdout_handle)?;
    let stderr = join_process_reader(stderr_handle)?;

    Ok(TimedProcessOutput {
        output: ProcessOutput {
            status,
            stdout,
            stderr,
        },
        timed_out,
        wall_us: start.elapsed().as_micros(),
    })
}

pub(crate) fn read_process_stream(mut stream: impl Read) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    stream.read_to_end(&mut output)?;
    Ok(output)
}

pub(crate) fn join_process_reader(
    handle: thread::JoinHandle<io::Result<Vec<u8>>>,
) -> io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| io::Error::other("process output reader panicked"))?
}

#[cfg(unix)]
pub(crate) fn configure_timeout_command(command: &mut ProcessCommand) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
}

#[cfg(not(unix))]
pub(crate) fn configure_timeout_command(_command: &mut ProcessCommand) {}

pub(crate) fn kill_timed_out_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pgid = child.id() as libc::pid_t;
        if pgid > 0 {
            let killed_group = unsafe { libc::kill(-pgid, libc::SIGKILL) } == 0;
            if killed_group {
                return;
            }
        }
    }

    let _ = child.kill();
}

pub(crate) fn stdout_line_count(stdout: &[u8]) -> usize {
    String::from_utf8_lossy(stdout).lines().count()
}

pub(crate) fn stdout_word_count(stdout: &[u8]) -> usize {
    String::from_utf8_lossy(stdout).split_whitespace().count()
}

pub(crate) fn stderr_preview(stderr: &[u8]) -> Option<String> {
    const MAX_CHARS: usize = 500;

    (!stderr.is_empty()).then(|| {
        String::from_utf8_lossy(stderr)
            .chars()
            .take(MAX_CHARS)
            .collect()
    })
}
