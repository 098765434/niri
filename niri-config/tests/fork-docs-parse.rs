//! The fork's counterpart of `wiki-parses.rs`: every ```kdl block under
//! docs/fork/ must parse (or fail, if marked `must-fail`). Kept in its own file
//! so the upstream test stays untouched.

use std::fs;
use std::path::{Path, PathBuf};

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            md_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

#[test]
fn fork_docs_parse() {
    let fork_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/fork");
    let mut files = vec![];
    md_files(&fork_dir, &mut files);

    let mut errors = vec![];
    let mut blocks = 0;

    for file in files {
        let contents = fs::read_to_string(&file).unwrap();
        let mut lines = contents
            .lines()
            .map(|line| line.trim().trim_start_matches('>').trim_start())
            .enumerate();

        while let Some((line_number, line)) = lines.next() {
            if !line.starts_with("```kdl") {
                continue;
            }
            let must_fail = line.contains("must-fail");

            let mut code = String::new();
            for (_, line) in lines.by_ref().take_while(|(_, line)| !line.starts_with("```")) {
                code.push_str(line);
                code.push('\n');
            }
            blocks += 1;

            let at = format!("{}:{}", file.display(), line_number + 1);
            match niri_config::Config::parse(&file, &code).config {
                Err(error) if !must_fail => errors.push(format!(
                    "Error parsing KDL block at {at}: {:?}",
                    miette::Report::new(error)
                )),
                Ok(_) if must_fail => errors.push(format!("Expected an error at {at}")),
                _ => (),
            }
        }
    }

    assert!(blocks > 0, "no ```kdl blocks found under docs/fork");
    assert!(
        errors.is_empty(),
        "Errors parsing {} fork doc KDL blocks:\n{}",
        errors.len(),
        errors.join("\n")
    );
}
