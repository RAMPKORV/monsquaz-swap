//! A small run report for end-to-end tests.
//!
//! The report is written as JSON for tools and as a self-contained HTML file
//! for humans. A failed run keeps the report, the screenshots, and the event
//! log next to each other.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub status: CaseStatus,
    pub duration_ms: u64,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
}

impl CaseResult {
    pub fn pass(name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: CaseStatus::Pass,
            duration_ms,
            detail: String::new(),
            screenshots: Vec::new(),
        }
    }

    pub fn fail(name: impl Into<String>, duration_ms: u64, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CaseStatus::Fail,
            duration_ms,
            detail: detail.into(),
            screenshots: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn with_screenshot(mut self, path: impl Into<String>) -> Self {
        self.screenshots.push(path.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub suite: String,
    pub started_at: i64,
    pub finished_at: i64,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub cases: Vec<CaseResult>,
}

impl RunReport {
    pub fn new(suite: impl Into<String>, started_at: i64) -> Self {
        Self {
            suite: suite.into(),
            started_at,
            finished_at: started_at,
            events: Vec::new(),
            cases: Vec::new(),
        }
    }

    pub fn record(&mut self, case: CaseResult) {
        self.cases.push(case);
    }

    pub fn event(&mut self, event: impl Into<String>) {
        self.events.push(event.into());
    }

    pub fn finish(mut self, finished_at: i64) -> Self {
        self.finished_at = finished_at;
        self
    }

    pub fn passed(&self) -> usize {
        self.count(CaseStatus::Pass)
    }

    pub fn failed(&self) -> usize {
        self.count(CaseStatus::Fail)
    }

    pub fn skipped(&self) -> usize {
        self.count(CaseStatus::Skip)
    }

    pub fn is_success(&self) -> bool {
        self.failed() == 0
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, json)
    }

    pub fn write_html(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_html())
    }

    pub fn to_html(&self) -> String {
        let mut rows = String::new();
        for case in &self.cases {
            let status = match case.status {
                CaseStatus::Pass => "pass",
                CaseStatus::Fail => "fail",
                CaseStatus::Skip => "skip",
            };
            let screenshots = case
                .screenshots
                .iter()
                .map(|path| {
                    format!(
                        "<a href=\"{}\">{}</a>",
                        escape(path),
                        escape(path.rsplit('/').next().unwrap_or(path))
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            rows.push_str(&format!(
                "<tr class=\"{status}\"><td>{}</td><td>{status}</td><td>{} ms</td><td>{}</td><td>{screenshots}</td></tr>\n",
                escape(&case.name),
                case.duration_ms,
                escape(&case.detail),
            ));
        }
        let events = self
            .events
            .iter()
            .map(|event| format!("<li>{}</li>", escape(event)))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Monsquaz Swap E2E: {suite}</title>
<style>
:root {{ color-scheme: dark; --ink: #f4f1ea; --paper: #14120f; --line: #3a352d; --pass: #6fcf97; --fail: #ff7a7a; --skip: #c9b458; }}
body {{ margin: 0; padding: 2rem; background: var(--paper); color: var(--ink); font: 15px/1.5 ui-monospace, monospace; }}
h1 {{ font-size: 1.4rem; margin: 0 0 .25rem; }}
p {{ margin: .25rem 0 1.5rem; color: #b9b1a3; }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ text-align: left; padding: .5rem .6rem; border-bottom: 1px solid var(--line); vertical-align: top; }}
th {{ color: #b9b1a3; font-weight: 600; }}
tr.pass td:nth-child(2) {{ color: var(--pass); }}
tr.fail td:nth-child(2) {{ color: var(--fail); }}
tr.skip td:nth-child(2) {{ color: var(--skip); }}
a {{ color: #9cc7ff; }}
ul {{ padding-left: 1.2rem; color: #b9b1a3; }}
</style>
</head>
<body>
<h1>Monsquaz Swap E2E</h1>
<p>Suite: {suite}. Started: {started}. Finished: {finished}. Passed: {passed}. Failed: {failed}. Skipped: {skipped}.</p>
<table>
<thead><tr><th>Case</th><th>Status</th><th>Duration</th><th>Detail</th><th>Screenshots</th></tr></thead>
<tbody>
{rows}</tbody>
</table>
<h2>Event log</h2>
<ul>
{events}
</ul>
</body>
</html>
"#,
            suite = escape(&self.suite),
            started = self.started_at,
            finished = self.finished_at,
            passed = self.passed(),
            failed = self.failed(),
            skipped = self.skipped(),
            rows = rows,
            events = events,
        )
    }

    fn count(&self, status: CaseStatus) -> usize {
        self.cases
            .iter()
            .filter(|case| case.status == status)
            .count()
    }
}

fn escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            other => output.push(other),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_counts_and_renders_escaped_cases() {
        let mut report = RunReport::new("classic", 10);
        report.record(CaseResult::pass("seed <intake>", 12));
        report.record(CaseResult::fail("reveal", 4, "channel missing"));
        report.event("roster locked");
        let finished = report.finish(20);
        assert_eq!(finished.passed(), 1);
        assert_eq!(finished.failed(), 1);
        assert!(!finished.is_success());
        let html = finished.to_html();
        assert!(html.contains("seed &lt;intake&gt;"));
        assert!(html.contains("channel missing"));
        assert!(html.contains("roster locked"));
    }

    #[test]
    fn a_report_round_trips_through_json() {
        let mut report = RunReport::new("async", 1);
        report.record(CaseResult::pass("wave one", 5).with_detail("two claims"));
        let json = serde_json::to_string(&report).unwrap();
        let decoded: RunReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cases[0].name, "wave one");
        assert_eq!(decoded.cases[0].status, CaseStatus::Pass);
    }

    #[test]
    fn a_report_writes_both_files() {
        let directory = tempfile::tempdir().unwrap();
        let report = RunReport::new("write", 1).finish(2);
        report
            .write_json(directory.path().join("nested/report.json"))
            .unwrap();
        report
            .write_html(directory.path().join("nested/report.html"))
            .unwrap();
        assert!(directory.path().join("nested/report.json").exists());
        assert!(directory.path().join("nested/report.html").exists());
    }
}
