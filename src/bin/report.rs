use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, ExitCode, Output, Stdio},
    time::Instant,
};

const RUNS: u32 = 5;
type ReportResult<T> = Result<T, Box<dyn Error>>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("report: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> ReportResult<()> {
    let usage = "usage: cargo aoc-report <N|dayN|baseline>";
    let mut args = env::args_os().skip(1);
    let target_arg = args.next();
    let target_arg = target_arg.as_deref().and_then(OsStr::to_str).ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }
    let target = if target_arg == "baseline" {
        target_arg.to_owned()
    } else {
        let day = target_arg.strip_prefix("day").unwrap_or(target_arg);
        if day.is_empty() || !day.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(usage.into());
        }
        format!("day{day}")
    };
    let target = target.as_str();
    if !cfg!(target_os = "macos") {
        return Err(
            "report generation requires macOS with /usr/bin/time and /usr/bin/leaks".into(),
        );
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input = if target == "baseline" {
        None
    } else {
        let path = root.join(format!("input/{target}.txt"));
        Some(
            path.canonicalize()
                .map_err(|error| format!("could not resolve input {}: {error}", path.display()))?,
        )
    };
    let build_dir = root.join("target/report-build");
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "build",
            "--release",
            "--bin",
            target,
            "--target",
            env!("REPORT_HOST"),
            "--target-dir",
        ])
        .arg(&build_dir)
        .status()?;
    if !status.success() {
        return Err(format!("release build failed for {target} ({status})").into());
    }
    let binary = build_dir
        .join(env!("REPORT_HOST"))
        .join("release")
        .join(target);

    let mut total_seconds = 0.0;
    for _ in 0..RUNS {
        let start = Instant::now();
        let status = Command::new(&binary)
            .args(input.iter())
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(
                format!("{target} exited with {status}. Run it directly for details.").into(),
            );
        }
        total_seconds += start.elapsed().as_secs_f64();
    }

    let mut peak_bytes = 0;
    for _ in 0..RUNS {
        let output = measurement(root, "/usr/bin/time", &["-l"], &binary, input.as_deref())?;
        if !output.status.success() {
            return Err(format!("/usr/bin/time -l failed ({})", output.status).into());
        }
        // macOS time reports maximum resident set size in bytes.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let bytes = stderr
            .lines()
            .rev()
            .find_map(|line| line.trim().strip_suffix("maximum resident set size"))
            .and_then(|value| value.trim().parse::<u64>().ok())
            .ok_or("/usr/bin/time -l did not report maximum resident set size")?;
        peak_bytes = peak_bytes.max(bytes);
    }

    let output = measurement(
        root,
        "/usr/bin/leaks",
        &["--noContent", "--atExit", "--"],
        &binary,
        input.as_deref(),
    )?;
    let leak_output = [
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ];
    let (leak_report, leak_count) = leak_output
        .iter()
        .find_map(|stream| leak_report(stream))
        .ok_or_else(|| format!("leaks did not produce a summary ({})", output.status))?;
    // leaks exits 1 when it finds leaks and uses other nonzero codes for errors.
    if !output.status.success() && (output.status.code() != Some(1) || leak_count == 0) {
        return Err(format!("leaks failed ({})", output.status).into());
    }

    let mean_ms = total_seconds / f64::from(RUNS) * 1000.0;
    let peak_kb = peak_bytes as f64 / 1000.0;
    let report = format!(
        "_This report was automatically generated via `cargo aoc-report {target}`._\n\n\
         # Report for {target}\n\n\
         ## Performance over {RUNS} runs measured with `Instant`\n\n\
         - **Average wall-clock time**: {mean_ms:.2} ms\n\n\
         ## Memory over {RUNS} runs measured with `time -l`\n\n\
         - **Peak RSS**: {peak_kb:.2} KB\n\n\
         ## Leak report measured with `leaks --noContent --atExit`\n\n\
         ```text\n{leak_report}\n```\n"
    );

    let directory = root.join("report");
    fs::create_dir_all(&directory)?;
    let report_name = if target == "baseline" {
        "0_baseline"
    } else {
        target
    };
    write_report(&directory, report_name, &report)?;
    println!("Wrote report/{report_name}.md");
    Ok(())
}

fn measurement(
    root: &Path,
    tool: &str,
    flags: &[&str],
    binary: &Path,
    input: Option<&Path>,
) -> ReportResult<Output> {
    Ok(Command::new(tool)
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(flags)
        .arg(binary)
        .args(input)
        .output()?)
}

fn leak_report(output: &str) -> Option<(String, u64)> {
    let mut lines = output.lines().map(str::trim).rev();
    let (totals, process, count) =
        lines.find_map(|line| leak_summary(line).map(|(process, count)| (line, process, count)))?;
    let mut malloc = None;
    let mut version = None;
    for line in lines {
        if line.starts_with("leaks Report Version: ") {
            version = Some(line);
            break;
        }
        if let Some((pid, summary)) = line.split_once(": ") {
            if pid == process && summary.contains(" nodes malloced for ") {
                malloc = Some(line);
            }
        }
    }
    let report = version
        .into_iter()
        .chain(malloc)
        .chain([totals])
        .collect::<Vec<_>>()
        .join("\n");
    Some((report, count))
}

fn leak_summary(line: &str) -> Option<(&str, u64)> {
    let (prefix, summary) = line.split_once(": ")?;
    let process = prefix.strip_prefix("Process ")?;
    process.parse::<u32>().ok()?;
    let (count, bytes) = summary
        .strip_suffix(" total leaked bytes.")?
        .split_once(" for ")?;
    let count = count
        .strip_suffix(" leaks")
        .or_else(|| count.strip_suffix(" leak"))?;
    bytes.parse::<u64>().ok()?;
    Some((prefix, count.parse().ok()?))
}

fn write_report(directory: &Path, name: &str, report: &str) -> std::io::Result<()> {
    let temporary = directory.join(format!(".{name}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = file.write_all(report.as_bytes());
    drop(file);
    let result =
        written.and_then(|()| fs::rename(&temporary, directory.join(format!("{name}.md"))));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{leak_report, leak_summary};

    #[test]
    fn retains_the_version_and_matching_malloc_summary() {
        let report = "leaks Report Version: 4.0, multi-line stacks\n\
                      Process 123: 187 nodes malloced for 106 KB\n\
                      Process 123: 1 leak for 32 total leaked bytes.";
        let output = format!(
            "Puzzle output\nProcess: example\nReport Version: 7\n{report}\nLeak stack trace\n"
        );
        assert_eq!(leak_report(&output), Some((report.to_owned(), 1)));
    }

    #[test]
    fn parses_leak_totals_without_retaining_diagnostic_text() {
        assert_eq!(
            leak_summary("Process 123: 0 leaks for 0 total leaked bytes."),
            Some(("Process 123", 0))
        );
        assert_eq!(
            leak_summary("Process 123: 1 leak for 32 total leaked bytes."),
            Some(("Process 123", 1))
        );
        assert_eq!(
            leak_summary("Process 123: 2 leaks for 96 total leaked bytes."),
            Some(("Process 123", 2))
        );
        assert_eq!(
            leak_summary("Process 123: 187 nodes malloced for 106 KB"),
            None
        );
    }
}
