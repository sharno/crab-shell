use std::{
    fs,
    sync::{Arc, Mutex},
};

use qshr::{pipeline, prelude::*, qshr};
use tempfile::tempdir;

#[test]
fn macro_runs_full_workflow() -> qshr::Result<()> {
    let temp = tempdir()?;
    let log = temp.path().join("macro.log");
    let nested_file = temp.path().join("nested.txt");
    let hits = Arc::new(Mutex::new(Vec::new()));
    let hits_a = hits.clone();
    let hits_b = hits.clone();
    let cwd = std::env::current_dir()?;

    qshr! {
        println!("running macro workflow");
        env "QSHR_MACRO_SCRIPT" = "1";

        let capture = cmd!("sh", "-c", "echo macro-run").stdout_text()?;
        write_text(&log, capture.trim().as_bytes())?;

        "echo literal pipeline" | "wc -w";
        let builder = pipeline!(sh("echo builder stage") | "tr a-z A-Z");
        run builder;

        cd(temp.path()) {
            write_text(&nested_file, "cd scope")?;
        };

        parallel {
            let mut guard = hits_a.lock().unwrap();
            guard.push(String::from("left"));
        } {
            let mut guard = hits_b.lock().unwrap();
            guard.push(String::from("right"));
        };

        unset "QSHR_MACRO_SCRIPT";
    }?;

    assert!(fs::read_to_string(&log)?.contains("macro-run"));
    assert!(nested_file.exists());
    assert_eq!(hits.lock().unwrap().len(), 2);
    assert_eq!(std::env::current_dir()?, cwd);
    assert!(std::env::var("QSHR_MACRO_SCRIPT").is_err());
    Ok(())
}

#[test]
fn macro_supports_control_flow() -> qshr::Result<()> {
    let temp = tempdir()?;
    let summary = temp.path().join("summary.txt");

    qshr! {
        let items = ["alpha", "beta", "gamma"];
        write_text(&summary, "")?;
        for item in &items {
            let output = cmd!("sh", "-c", &format!("echo {}", item)).stdout_text()?;
            append_text(&summary, format!("{}\n", output.trim().len()))?;
        };

        let collector = pipeline!(sh("echo left") | "cat")
            .pipe(sh("tr a-z A-Z"));
        run collector;
    }?;

    let summary_lines = fs::read_to_string(&summary)?;
    assert_eq!(
        summary_lines.lines().collect::<Vec<_>>(),
        vec!["5", "4", "5"]
    );
    Ok(())
}
