use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::models::{Branch, CommitInfo, FileStatus, GitStatus};

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn run_git_cmd(dir: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    cmd.args(args);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute 'git {}'", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Process exited with status {:?}", output.status.code())
        };
        return Err(anyhow!(msg));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn is_git_repository(dir: &Path) -> bool {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    cmd.args(["rev-parse", "--is-inside-work-tree"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(output) = cmd.output() {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).trim() == "true"
    } else {
        false
    }
}

pub fn get_repo_root(dir: &Path) -> Result<PathBuf> {
    let out = run_git_cmd(dir, &["rev-parse", "--show-toplevel"])?;
    let path_str = out.trim();
    Ok(PathBuf::from(path_str))
}

pub fn get_current_branch(dir: &Path) -> Result<String> {
    let out = run_git_cmd(dir, &["branch", "--show-current"])?;
    let branch = out.trim();
    if branch.is_empty() {
        // Might be detached HEAD
        let head_out = run_git_cmd(dir, &["rev-parse", "--short", "HEAD"])?;
        let head = head_out.trim();
        if head.is_empty() {
            Ok("HEAD (empty)".to_string())
        } else {
            Ok(format!("HEAD ({})", head))
        }
    } else {
        Ok(branch.to_string())
    }
}

pub fn list_branches(dir: &Path) -> Result<Vec<Branch>> {
    let out = run_git_cmd(
        dir,
        &[
            "branch",
            "-a",
            "--format=%(refname:short)|%(HEAD)|%(upstream:short)",
        ],
    )?;

    let mut branches = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.is_empty() || parts[0].trim().is_empty() {
            continue;
        }

        let name = parts[0].trim().to_string();
        if name.contains("HEAD detached") || name.ends_with("/HEAD") {
            continue;
        }

        let is_current = parts.get(1).map(|&s| s.trim() == "*").unwrap_or(false);
        let upstream = parts
            .get(2)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let is_remote = name.starts_with("origin/") || name.contains('/');

        branches.push(Branch {
            name,
            is_current,
            is_remote,
            upstream,
        });
    }

    Ok(branches)
}

pub fn create_branch(dir: &Path, name: &str, checkout: bool) -> Result<()> {
    if checkout {
        run_git_cmd(dir, &["checkout", "-b", name])?;
    } else {
        run_git_cmd(dir, &["branch", name])?;
    }
    Ok(())
}

pub fn checkout_branch(dir: &Path, name: &str) -> Result<()> {
    // If it's a remote branch like origin/feature, strip remote prefix to track it locally
    if let Some(local_name) = name.strip_prefix("origin/") {
        let _ = run_git_cmd(dir, &["checkout", "-B", local_name, "--track", name])
            .or_else(|_| run_git_cmd(dir, &["checkout", local_name]));
        return Ok(());
    }
    run_git_cmd(dir, &["checkout", name])?;
    Ok(())
}

pub fn delete_branch(dir: &Path, name: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    run_git_cmd(dir, &["branch", flag, name])?;
    Ok(())
}

pub fn get_status(dir: &Path) -> Result<GitStatus> {
    let out = run_git_cmd(dir, &["status", "--porcelain=v1", "-uall"])?;
    let mut status = GitStatus::default();

    for line in out.lines() {
        if line.len() < 3 {
            continue;
        }

        let index_code = line.chars().next().unwrap_or(' ');
        let worktree_code = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].trim().trim_matches('"').to_string();

        if index_code == '?' && worktree_code == '?' {
            status.untracked.push(FileStatus {
                path,
                index_status: '?',
                worktree_status: '?',
                is_staged: false,
            });
            continue;
        }

        if index_code != ' ' && index_code != '?' {
            status.staged.push(FileStatus {
                path: path.clone(),
                index_status: index_code,
                worktree_status: ' ',
                is_staged: true,
            });
        }

        if worktree_code != ' ' && worktree_code != '?' {
            status.unstaged.push(FileStatus {
                path,
                index_status: ' ',
                worktree_status: worktree_code,
                is_staged: false,
            });
        }
    }

    Ok(status)
}

pub fn stage_files(dir: &Path, files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--"];
    for f in files {
        args.push(f.as_str());
    }
    run_git_cmd(dir, &args)?;
    Ok(())
}

pub fn stage_all(dir: &Path) -> Result<()> {
    run_git_cmd(dir, &["add", "-A"])?;
    Ok(())
}

pub fn unstage_files(dir: &Path, files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["restore", "--staged", "--"];
    for f in files {
        args.push(f.as_str());
    }
    run_git_cmd(dir, &args)?;
    Ok(())
}

pub fn unstage_all(dir: &Path) -> Result<()> {
    run_git_cmd(dir, &["restore", "--staged", "."])?;
    Ok(())
}

pub fn discard_file_changes(dir: &Path, file: &str, is_untracked: bool) -> Result<()> {
    if is_untracked {
        let full_path = dir.join(file);
        if full_path.is_file() {
            std::fs::remove_file(full_path)?;
        } else if full_path.is_dir() {
            std::fs::remove_dir_all(full_path)?;
        }
    } else {
        run_git_cmd(dir, &["restore", "--", file])?;
    }
    Ok(())
}

pub fn commit(dir: &Path, message: &str, amend: bool) -> Result<()> {
    let mut args = vec!["commit", "-m", message];
    if amend {
        args.push("--amend");
    }
    run_git_cmd(dir, &args)?;
    Ok(())
}

pub fn get_log(dir: &Path, count: usize) -> Result<Vec<CommitInfo>> {
    let count_str = count.to_string();
    let format_arg = "--pretty=format:%H%x00%h%x00%an%x00%ae%x00%ad%x00%s%x00%b%x1f";
    let out = match run_git_cmd(
        dir,
        &["log", "-n", &count_str, format_arg, "--date=short"],
    ) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()), // Repo might have 0 commits yet
    };

    let mut commits = Vec::new();
    for commit_chunk in out.split('\u{001f}') {
        let chunk = commit_chunk.trim();
        if chunk.is_empty() {
            continue;
        }

        let parts: Vec<&str> = chunk.split('\u{0000}').collect();
        if parts.len() >= 6 {
            let hash = parts[0].trim().to_string();
            let short_hash = parts[1].trim().to_string();
            let author_name = parts[2].trim().to_string();
            let author_email = parts[3].trim().to_string();
            let date = parts[4].trim().to_string();
            let summary = parts[5].trim().to_string();
            let body = parts.get(6).map(|&b| b.trim().to_string()).unwrap_or_default();

            commits.push(CommitInfo {
                hash,
                short_hash,
                author_name,
                author_email,
                date,
                summary,
                body,
            });
        }
    }

    Ok(commits)
}

pub fn get_diff(dir: &Path, file: Option<&str>, staged: bool) -> Result<String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    if let Some(f) = file {
        args.push("--");
        args.push(f);
    }
    run_git_cmd(dir, &args)
}

pub fn get_untracked_diff(dir: &Path, file: &str) -> Result<String> {
    let full_path = dir.join(file);
    if !full_path.exists() {
        return Ok(String::new());
    }

    if let Ok(content) = std::fs::read_to_string(&full_path) {
        let mut diff = format!("diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1,{1} @@\n", file, content.lines().count());
        for line in content.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        Ok(diff)
    } else {
        Ok(format!("Binary file {} differs\n", file))
    }
}

pub fn get_commit_diff(dir: &Path, hash: &str) -> Result<String> {
    run_git_cmd(dir, &["show", "--format=", hash])
}

pub fn get_commit_files(dir: &Path, hash: &str) -> Result<Vec<String>> {
    let out = run_git_cmd(dir, &["diff-tree", "--no-commit-id", "--name-only", "-r", hash])?;
    Ok(out.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}

pub fn fetch(dir: &Path) -> Result<String> {
    run_git_cmd(dir, &["fetch", "--all", "--prune"])
}

pub fn pull(dir: &Path) -> Result<String> {
    run_git_cmd(dir, &["pull"])
}

pub fn push(dir: &Path) -> Result<String> {
    run_git_cmd(dir, &["push"])
}

pub fn init_repo(dir: &Path) -> Result<()> {
    run_git_cmd(dir, &["init"])?;
    Ok(())
}

pub fn read_file_content(dir: &Path, rel_path: &str) -> Result<String> {
    let full_path = dir.join(rel_path);
    std::fs::read_to_string(full_path).map_err(|e| anyhow!("Failed to read file: {}", e))
}
