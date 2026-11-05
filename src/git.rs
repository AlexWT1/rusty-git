use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::models::{Branch, CommitInfo, FileStatus, GitStatus, RemoteInfo};

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
        // Might be detached HEAD or empty unborn branch
        let head_out = run_git_cmd(dir, &["rev-parse", "--short", "HEAD"]);
        match head_out {
            Ok(h) => {
                let head = h.trim();
                if head.is_empty() {
                    Ok("main (unborn)".to_string())
                } else {
                    Ok(format!("HEAD ({})", head))
                }
            }
            Err(_) => Ok("main (unborn)".to_string()),
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
    let out = run_git_cmd(
        dir,
        &["-c", "core.quotepath=false", "status", "--porcelain=v1", "-uall"],
    )?;
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
    // Always operate relative to repository root to avoid pathspec mismatches
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());

    let mut clean_files = Vec::new();
    for f in files {
        if let Some((_old_p, new_p)) = f.split_once(" -> ") {
            clean_files.push(new_p.trim().trim_matches('"').to_string());
        } else {
            clean_files.push(f.trim().trim_matches('"').to_string());
        }
    }

    let mut args = vec!["add", "--"];
    for f in &clean_files {
        args.push(f.as_str());
    }
    run_git_cmd(&root, &args)?;
    Ok(())
}

pub fn stage_all(dir: &Path) -> Result<()> {
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());
    run_git_cmd(&root, &["add", "-A"])?;
    Ok(())
}

pub fn unstage_files(dir: &Path, files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    // Always operate relative to repository root to avoid pathspec mismatches
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());

    // Handle renamed files ("old -> new") and clean paths
    let mut clean_files = Vec::new();
    for f in files {
        if let Some((old_p, new_p)) = f.split_once(" -> ") {
            clean_files.push(old_p.trim().trim_matches('"').to_string());
            clean_files.push(new_p.trim().trim_matches('"').to_string());
        } else {
            clean_files.push(f.trim().trim_matches('"').to_string());
        }
    }

    // git reset -q -- <files> unstages files in all repository states (including unborn/empty branch)
    let mut reset_args = vec!["reset", "-q", "--"];
    for f in &clean_files {
        reset_args.push(f.as_str());
    }

    if let Err(e) = run_git_cmd(&root, &reset_args) {
        let mut restore_args = vec!["restore", "--staged", "--"];
        for f in &clean_files {
            restore_args.push(f.as_str());
        }
        if let Err(e2) = run_git_cmd(&root, &restore_args) {
            let mut rm_args = vec!["rm", "--cached", "-r", "--"];
            for f in &clean_files {
                rm_args.push(f.as_str());
            }
            run_git_cmd(&root, &rm_args)
                .with_context(|| format!("Failed to unstage files: {} / {}", e, e2))?;
        }
    }
    Ok(())
}

pub fn unstage_all(dir: &Path) -> Result<()> {
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());
    // git reset -q unstages the entire index in both unborn/empty repos and repos with commits
    if let Err(e) = run_git_cmd(&root, &["reset", "-q"]) {
        if let Err(e2) = run_git_cmd(&root, &["restore", "--staged", "."]) {
            run_git_cmd(&root, &["rm", "--cached", "-r", "."])
                .with_context(|| format!("Failed to unstage all: {} / {}", e, e2))?;
        }
    }
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

pub fn list_remotes(dir: &Path) -> Result<Vec<RemoteInfo>> {
    let out = match run_git_cmd(dir, &["remote", "-v"]) {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };
    let mut remotes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let url = parts[1].to_string();
            if seen.insert(name.clone()) {
                remotes.push(RemoteInfo { name, url });
            }
        }
    }
    Ok(remotes)
}

pub fn add_or_set_remote(dir: &Path, name: &str, url: &str) -> Result<()> {
    let existing = list_remotes(dir).unwrap_or_default();
    if existing.iter().any(|r| r.name == name) {
        run_git_cmd(dir, &["remote", "set-url", name, url])?;
    } else {
        run_git_cmd(dir, &["remote", "add", name, url])?;
    }
    Ok(())
}

pub fn remove_remote(dir: &Path, name: &str) -> Result<()> {
    run_git_cmd(dir, &["remote", "remove", name])?;
    Ok(())
}

pub fn push_set_upstream(dir: &Path, remote: &str, branch: &str) -> Result<String> {
    run_git_cmd(dir, &["push", "-u", remote, branch])
}

pub fn read_gitignore(dir: &Path) -> Result<String> {
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());
    let path = root.join(".gitignore");
    if path.exists() {
        std::fs::read_to_string(path).map_err(|e| anyhow!("Failed to read .gitignore: {}", e))
    } else {
        Ok(String::new())
    }
}

pub fn save_gitignore(dir: &Path, content: &str) -> Result<()> {
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());
    let path = root.join(".gitignore");
    std::fs::write(path, content).map_err(|e| anyhow!("Failed to save .gitignore: {}", e))
}

#[allow(dead_code)]
pub fn append_to_gitignore(dir: &Path, pattern: &str) -> Result<()> {
    let root = get_repo_root(dir).unwrap_or_else(|_| dir.to_path_buf());
    let path = root.join(".gitignore");
    let mut current = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    if !current.ends_with('\n') && !current.is_empty() {
        current.push('\n');
    }
    current.push_str(pattern.trim());
    current.push('\n');
    std::fs::write(path, current).map_err(|e| anyhow!("Failed to append to .gitignore: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unstage_in_empty_and_normal_repo() {
        let temp_dir = std::env::temp_dir().join(format!("git_test_unstage_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Unborn / empty repo test
        init_repo(&temp_dir).unwrap();
        let file_path = temp_dir.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        // Stage file
        stage_files(&temp_dir, &["test.txt".to_string()]).unwrap();
        let status = get_status(&temp_dir).unwrap();
        assert_eq!(status.staged.len(), 1, "Should have 1 staged file in unborn repo");
        assert_eq!(status.staged[0].path, "test.txt");

        // Unstage single file
        unstage_files(&temp_dir, &["test.txt".to_string()]).unwrap();
        let status = get_status(&temp_dir).unwrap();
        assert_eq!(status.staged.len(), 0, "Staged should be empty after unstage_files");
        assert_eq!(status.untracked.len(), 1, "File should be untracked");

        // Stage file again and test unstage_all
        stage_files(&temp_dir, &["test.txt".to_string()]).unwrap();
        let status = get_status(&temp_dir).unwrap();
        assert_eq!(status.staged.len(), 1);

        unstage_all(&temp_dir).unwrap();
        let status = get_status(&temp_dir).unwrap();
        assert_eq!(status.staged.len(), 0, "Staged should be empty after unstage_all");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stage_file_with_spaces_from_subdirectory() {
        let temp_dir = std::env::temp_dir().join(format!("git_test_spaces_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        init_repo(&temp_dir).unwrap();

        // Create file with space in name
        let file_path = temp_dir.join("Text Document.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        // Create a subdirectory inside repo
        let sub_dir = temp_dir.join("subfolder").join("nested");
        std::fs::create_dir_all(&sub_dir).unwrap();

        // Staging from sub_dir should resolve to repo root and stage "Text Document.txt" without pathspec errors!
        let status = get_status(&sub_dir).unwrap();
        assert_eq!(status.untracked.len(), 1);
        assert_eq!(status.untracked[0].path, "Text Document.txt");

        stage_files(&sub_dir, &[status.untracked[0].path.clone()]).expect("Should stage file with spaces even from subdirectory");

        let status = get_status(&sub_dir).unwrap();
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].path, "Text Document.txt");

        unstage_files(&sub_dir, &[status.staged[0].path.clone()]).expect("Should unstage file with spaces even from subdirectory");

        let status = get_status(&sub_dir).unwrap();
        assert_eq!(status.staged.len(), 0);
        assert_eq!(status.untracked.len(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_gitignore_read_save_append() {
        let temp_dir = std::env::temp_dir().join(format!("git_test_ignore_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        init_repo(&temp_dir).unwrap();

        // Reading non-existent .gitignore should return empty string
        let content = read_gitignore(&temp_dir).unwrap();
        assert_eq!(content, "");

        // Save content
        save_gitignore(&temp_dir, "*.log\ntarget/\n").unwrap();
        let content = read_gitignore(&temp_dir).unwrap();
        assert_eq!(content, "*.log\ntarget/\n");

        // Append pattern
        append_to_gitignore(&temp_dir, ".env").unwrap();
        let content = read_gitignore(&temp_dir).unwrap();
        assert_eq!(content, "*.log\ntarget/\n.env\n");

        // Calling from subdirectory should still edit repo root's .gitignore
        let sub = temp_dir.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        append_to_gitignore(&sub, "build/").unwrap();
        let content = read_gitignore(&sub).unwrap();
        assert!(content.contains("build/\n"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
