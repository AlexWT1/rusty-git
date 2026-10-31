#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: String,
    pub index_status: char,
    pub worktree_status: char,
    pub is_staged: bool,
}

impl FileStatus {
    pub fn display_status(&self) -> &str {
        if self.is_staged {
            match self.index_status {
                'M' => "Modified",
                'A' => "Added",
                'D' => "Deleted",
                'R' => "Renamed",
                'C' => "Copied",
                '?' => "Untracked",
                _ => "Changed",
            }
        } else {
            match self.worktree_status {
                'M' => "Modified",
                'D' => "Deleted",
                '?' => "Untracked",
                _ => "Modified",
            }
        }
    }

    pub fn status_color(&self) -> (u8, u8, u8) {
        if self.is_staged {
            (100, 220, 100) // Greenish
        } else {
            match self.worktree_status {
                '?' => (130, 180, 255), // Blue/cyan untracked
                'D' => (240, 100, 100), // Red deleted
                _ => (250, 180, 80),   // Orange modified
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub staged: Vec<FileStatus>,
    pub unstaged: Vec<FileStatus>,
    pub untracked: Vec<FileStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
    pub summary: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    HunkHeader,
    Meta,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedDiff {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
}
