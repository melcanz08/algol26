// tests/docs_examples.rs
//
// Blocks in docs/language-reference.md marked with a first-line
// `// test:compile` comment are extracted and run through
// `algol26 check`. Any marked block that fails to compile is a
// test failure.
//
// Unmarked blocks are ignored — they may be fragments, diagrams,
// placeholders, or intentional negative examples.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn extract_gol_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current_block: Vec<String> = Vec::new();
    let mut block_start: usize = 0;
    let mut in_block = false;

    for (line_no, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if !in_block {
                let lang = rest.trim().to_lowercase();
                let is_algol26 =
                    lang.is_empty() || lang == "gol" || lang == "algol26" || lang == "algol";
                if is_algol26 {
                    in_block = true;
                    block_start = line_no + 1;
                    current_block.clear();
                }
            } else {
                in_block = false;
                if !current_block.is_empty() {
                    blocks.push((block_start, current_block.join("\n")));
                }
                current_block.clear();
            }
        } else if in_block {
            current_block.push(line.to_string());
        }
    }

    blocks
}

fn is_marked_for_test(block: &str) -> bool {
    block
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim() == "// test:compile")
        .unwrap_or(false)
}

#[test]
fn language_reference_marked_examples_compile() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/language-reference.md");
    let markdown = fs::read_to_string(&path).expect("failed to read docs/language-reference.md");

    let all_blocks = extract_gol_blocks(&markdown);
    let marked: Vec<_> = all_blocks
        .into_iter()
        .filter(|(_, b)| is_marked_for_test(b))
        .collect();

    if marked.is_empty() {
        eprintln!(
            "docs_examples: no `// test:compile` marked blocks found — \
             skipping (mark blocks you want verified)"
        );
        return;
    }

    let bin = env!("CARGO_BIN_EXE_algol26");
    let tmp_dir = std::env::temp_dir().join("algol26_docs_examples");
    fs::create_dir_all(&tmp_dir).expect("failed to create temp dir");

    let mut failures: Vec<(usize, String, String)> = Vec::new();

    for (line_no, block) in &marked {
        let source: String = block.lines().skip(1).collect::<Vec<_>>().join("\n");

        let file_path = tmp_dir.join(format!("block_{}.gol", line_no));
        fs::write(&file_path, &source).expect("failed to write temp file");

        let output = Command::new(bin)
            .arg("check")
            .arg(&file_path)
            .output()
            .expect("failed to run algol26 check");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            failures.push((*line_no, source, format!("{}\n{}", stdout, stderr)));
        }
    }

    if !failures.is_empty() {
        let mut msg = format!(
            "{} marked docs examples in docs/language-reference.md \
             failed to compile:\n\n",
            failures.len()
        );
        for (line_no, source, err) in &failures {
            msg.push_str(&format!(
                "════ line {} ════\n--- source ---\n{}\n--- error ---\n{}\n\n",
                line_no, source, err
            ));
        }
        panic!("{}", msg);
    }

    eprintln!("docs_examples: {} marked blocks compiled", marked.len());
}
