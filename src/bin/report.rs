use std::{
    collections::HashSet,
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
const LEAK_DEMO_TARGET: &str = "day999";
const LEAK_DEMO_INPUT: &str = "FAKE-AOC-LEAK999";
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

    let leak_flags: &[_] = if target == LEAK_DEMO_TARGET {
        &["--list", "--fullContent", "--hex", "--atExit", "--"]
    } else {
        &["--noContent", "--atExit", "--"]
    };
    let output = measurement(
        root,
        "/usr/bin/leaks",
        leak_flags,
        &binary,
        input.as_deref(),
    )?;
    let leak_output = [
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ];
    let (mut leak_report, leak_count) =
        leak_output
            .iter()
            .find_map(|stream| leak_report(stream))
            .ok_or_else(|| format!("leaks did not produce a summary ({})", output.status))?;
    // leaks exits 1 when it finds leaks and uses other nonzero codes for errors.
    if !output.status.success() && (output.status.code() != Some(1) || leak_count == 0) {
        return Err(format!("leaks failed ({})", output.status).into());
    }
    if target == LEAK_DEMO_TARGET {
        let recovered = leak_output
            .iter()
            .flat_map(|stream| stream.lines())
            .filter_map(|line| line.split_once(" \t"))
            .map(|(_, ascii)| ascii.trim())
            .find(|ascii| *ascii == LEAK_DEMO_INPUT)
            .ok_or("leaks did not recover the fake input from the allocation")?;
        leak_report.push('\n');
        leak_report.push_str(recovered);
    }
    let leak_command = leak_flags.join(" ");

    let mean_ms = total_seconds / f64::from(RUNS) * 1000.0;
    let peak_kb = peak_bytes as f64 / 1000.0;
    let report = format!(
        "_This report was automatically generated via `cargo aoc-report {target}`._\n\n\
         # Report for {target}\n\n\
         ## Performance over {RUNS} runs measured with `Instant`\n\n\
         - **Average wall-clock time**: {mean_ms:.2} ms\n\n\
         ## Memory over {RUNS} runs measured with `time -l`\n\n\
         - **Peak RSS**: {peak_kb:.2} KB\n\n\
         ## Leak report measured with `leaks {leak_command}`\n\n\
         ```text\n{leak_report}\n```\n"
    );
    let report = match input {
        Some(path) => scrub_input(&report, &fs::read_to_string(path)?),
        None => report,
    };

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

fn scrub_input(report: &str, input: &str) -> String {
    const REDACTION: &str = "<Puzzle Input Scrubbed Automatically>";

    let input = input.replace("\r\n", "\n");
    let block = input.trim_end_matches('\n');
    let report = if block.contains('\n') {
        std::borrow::Cow::Owned(report.replace(block, REDACTION))
    } else {
        std::borrow::Cow::Borrowed(report)
    };
    let input_lines: HashSet<_> = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let mut scrubbed = String::with_capacity(report.len());
    for line in report.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if input_lines.contains(content) {
            scrubbed.push_str(REDACTION);
            if line.ends_with('\n') {
                scrubbed.push('\n');
            }
        } else {
            for (index, field) in line.split('"').enumerate() {
                if index != 0 {
                    scrubbed.push('"');
                }
                if index % 2 == 1 && input_lines.contains(field) {
                    scrubbed.push_str(REDACTION);
                } else {
                    scrubbed.push_str(field);
                }
            }
        }
    }
    scrubbed
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
    let (process, count, bytes) = lines.find_map(leak_summary)?;
    let mut malloc = None;
    let mut version = None;
    for line in lines {
        if let Some(value) = line.strip_prefix("leaks Report Version: ") {
            let number = value.strip_suffix(", multi-line stacks").unwrap_or(value);
            if number.split_once('.').is_some_and(|(major, minor)| {
                major.parse::<u32>().is_ok() && minor.parse::<u32>().is_ok()
            }) {
                version = Some(format!("leaks Report Version: {value}"));
            }
            break;
        }
        if let Some((nodes, size)) = malloc_summary(line, process) {
            malloc = Some(format!(
                "Process {process}: {nodes} nodes malloced for {size}"
            ));
        }
    }
    let noun = if count == 1 { "leak" } else { "leaks" };
    let totals = format!("Process {process}: {count} {noun} for {bytes} total leaked bytes.");
    let report = version
        .into_iter()
        .chain(malloc)
        .chain([totals])
        .collect::<Vec<_>>()
        .join("\n");
    Some((report, count))
}

fn malloc_summary(line: &str, process: u32) -> Option<(u64, &str)> {
    let (pid, summary) = line.strip_prefix("Process ")?.split_once(": ")?;
    if pid.parse::<u32>().ok()? != process {
        return None;
    }
    let (nodes, size) = summary.split_once(" nodes malloced for ")?;
    let (amount, unit) = size.split_once(' ')?;
    let valid_amount = amount.parse::<u64>().is_ok()
        || amount.split_once('.').is_some_and(|(whole, fraction)| {
            whole.parse::<u64>().is_ok() && fraction.parse::<u64>().is_ok()
        });
    if !valid_amount || !matches!(unit, "B" | "bytes" | "KB" | "MB" | "GB" | "TB") {
        return None;
    }
    Some((nodes.parse().ok()?, size))
}

fn leak_summary(line: &str) -> Option<(u32, u64, u64)> {
    let (prefix, summary) = line.split_once(": ")?;
    let process = prefix.strip_prefix("Process ")?.parse().ok()?;
    let (count, bytes) = summary
        .strip_suffix(" total leaked bytes.")?
        .split_once(" for ")?;
    let count = count
        .strip_suffix(" leaks")
        .or_else(|| count.strip_suffix(" leak"))?;
    Some((process, count.parse().ok()?, bytes.parse().ok()?))
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
    use super::{leak_report, leak_summary, scrub_input};

    #[test]
    fn scrubs_literal_input_blocks_and_individual_input_lines() {
        let report = "Header\n\nLeak: [A.*\nR827]\nR827\npayload=\"R827\"\nTotals\n";
        let input = "A.*\r\nR827\r\n";
        assert_eq!(
            scrub_input(report, input),
            "Header\n\nLeak: [<Puzzle Input Scrubbed Automatically>]\n<Puzzle Input Scrubbed Automatically>\npayload=\"<Puzzle Input Scrubbed Automatically>\"\nTotals\n"
        );
        assert_eq!(scrub_input(report, "\r\n"), report);
    }

    #[test]
    fn retains_the_version_and_matching_malloc_summary() {
        let report = "leaks Report Version: 4.0, multi-line stacks\n\
                      Process 123: 187 nodes malloced for 106 KB\n\
                      Process 123: 1 leak for 32 total leaked bytes.";
        let output = format!(
            "Puzzle output\nProcess: example\nReport Version: 7\n{report}\nLeak stack trace\n"
        );
        assert_eq!(leak_report(&output), Some((report.to_owned(), 1)));
        let malformed = "leaks Report Version: 4.0, unexpected contents\n\
                         Process 123: 187 nodes malloced for 106 KB, unexpected contents\n\
                         Process 123: 1 leak for 32 total leaked bytes.";
        assert_eq!(
            leak_report(malformed),
            Some((
                "Process 123: 1 leak for 32 total leaked bytes.".to_owned(),
                1
            ))
        );
    }

    #[test]
    fn parses_leak_totals_without_retaining_diagnostic_text() {
        assert_eq!(
            leak_summary("Process 123: 0 leaks for 0 total leaked bytes."),
            Some((123, 0, 0))
        );
        assert_eq!(
            leak_summary("Process 123: 1 leak for 32 total leaked bytes."),
            Some((123, 1, 32))
        );
        assert_eq!(
            leak_summary("Process 123: 2 leaks for 96 total leaked bytes."),
            Some((123, 2, 96))
        );
        assert_eq!(
            leak_summary("Process 123: 187 nodes malloced for 106 KB"),
            None
        );
    }
}
