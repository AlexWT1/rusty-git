use eframe::egui::{self, RichText, Vec2};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::diff_parser::parse_git_diff;
use crate::git;
use crate::models::{Branch, CommitInfo, GitStatus, ParsedDiff, RemoteInfo};
use crate::theme::widgets::Btn;
use crate::theme::{icons, metrics, tokens, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Changes,
    Branches,
    History,
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFile {
    pub path: String,
    pub is_staged: bool,
    pub is_untracked: bool,
}

enum AsyncMessage {
    RefreshDone {
        current_branch: String,
        branches: Vec<Branch>,
        remotes: Vec<RemoteInfo>,
        status: GitStatus,
        commits: Vec<CommitInfo>,
    },
    DiffLoaded {
        target: String,
        diff: ParsedDiff,
    },
    CommitDiffLoaded {
        hash: String,
        files: Vec<String>,
        diff: ParsedDiff,
    },
    FileContentLoaded {
        path: String,
        content: String,
    },
    OperationFinished {
        action_name: String,
        result: Result<String, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ToastAlert {
    pub title: String,
    pub message: String,
    pub is_error: bool,
    pub created_at: std::time::Instant,
}

pub struct RustyGitApp {
    repo_path: Option<PathBuf>,
    is_git_repo: bool,

    current_branch: String,
    branches: Vec<Branch>,
    remotes: Vec<RemoteInfo>,
    git_status: GitStatus,
    commits: Vec<CommitInfo>,

    selected_tab: MainTab,
    selected_file: Option<SelectedFile>,
    current_diff: Option<ParsedDiff>,

    selected_commit: Option<CommitInfo>,
    selected_commit_files: Vec<String>,
    selected_commit_diff: Option<ParsedDiff>,

    file_view_path: Option<String>,
    file_view_content: Option<String>,
    file_view_lines: Vec<String>,
    file_view_max_len: usize,
    dir_cache: std::collections::HashMap<PathBuf, (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>)>,

    commit_message: String,
    commit_amend: bool,

    search_query: String,

    // Async / threading
    tx: Sender<AsyncMessage>,
    rx: Receiver<AsyncMessage>,
    is_busy: bool,
    busy_message: String,

    // Notification toast & alert
    notification: Option<(String, bool, std::time::Instant)>,
    toast: Option<ToastAlert>,

    // Modals & confirmation dialogs
    show_new_branch_modal: bool,
    new_branch_name: String,
    new_branch_checkout: bool,

    show_connect_remote_modal: bool,
    remote_name_input: String,
    remote_url_input: String,
    remote_fetch_after: bool,
    remote_push_upstream: bool,

    show_gitignore_modal: bool,
    gitignore_content: String,
    gitignore_new_pattern: String,

    show_discard_dialog: Option<(String, bool)>,
    show_delete_branch_dialog: Option<String>,
}

impl RustyGitApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();

        // Default to repository root of current directory
        let current_dir = std::env::current_dir().ok();
        let actual_repo = current_dir
            .as_ref()
            .and_then(|p| git::get_repo_root(p).ok())
            .or_else(|| current_dir.clone());
        let is_repo = actual_repo
            .as_ref()
            .map(|p| git::is_git_repository(p))
            .unwrap_or(false);

        let mut app = Self {
            repo_path: actual_repo,
            is_git_repo: is_repo,
            current_branch: String::new(),
            branches: Vec::new(),
            remotes: Vec::new(),
            git_status: GitStatus::default(),
            commits: Vec::new(),
            selected_tab: MainTab::Changes,
            selected_file: None,
            current_diff: None,
            selected_commit: None,
            selected_commit_files: Vec::new(),
            selected_commit_diff: None,
            file_view_path: None,
            file_view_content: None,
            file_view_lines: Vec::new(),
            file_view_max_len: 0,
            dir_cache: std::collections::HashMap::new(),
            commit_message: String::new(),
            commit_amend: false,
            search_query: String::new(),
            tx,
            rx,
            is_busy: false,
            busy_message: String::new(),
            notification: None,
            toast: None,
            show_new_branch_modal: false,
            new_branch_name: String::new(),
            new_branch_checkout: true,
            show_connect_remote_modal: false,
            remote_name_input: "origin".to_string(),
            remote_url_input: String::new(),
            remote_fetch_after: true,
            remote_push_upstream: true,
            show_gitignore_modal: false,
            gitignore_content: String::new(),
            gitignore_new_pattern: String::new(),
            show_discard_dialog: None,
            show_delete_branch_dialog: None,
        };

        if app.is_git_repo {
            app.trigger_refresh();
        }

        app
    }

    fn notify_alert(&mut self, title: impl Into<String>, msg: impl Into<String>, is_error: bool) {
        let t = title.into();
        let m = msg.into();
        self.notification = Some((format!("{}: {}", t, m), is_error, std::time::Instant::now()));
        self.toast = Some(ToastAlert {
            title: t,
            message: m,
            is_error,
            created_at: std::time::Instant::now(),
        });
    }

    fn notify_success(&mut self, title: impl Into<String>, msg: impl Into<String>) {
        self.notify_alert(title, msg, false);
    }

    fn notify_error(&mut self, title: impl Into<String>, msg: impl Into<String>) {
        self.notify_alert(title, msg, true);
    }

    fn open_gitignore_modal(&mut self) {
        if let Some(repo) = &self.repo_path {
            match git::read_gitignore(repo) {
                Ok(content) => self.gitignore_content = content,
                Err(_) => self.gitignore_content = String::new(),
            }
            self.gitignore_new_pattern.clear();
            self.show_gitignore_modal = true;
        }
    }

    fn open_gitignore_modal_with_pattern(&mut self, pattern: &str) {
        self.open_gitignore_modal();
        self.gitignore_new_pattern = pattern.to_string();
    }

    pub fn set_repo(&mut self, path: PathBuf) {
        let actual_path = git::get_repo_root(&path).unwrap_or(path);
        let is_repo = git::is_git_repository(&actual_path);
        self.repo_path = Some(actual_path);
        self.is_git_repo = is_repo;
        self.selected_file = None;
        self.current_diff = None;
        self.selected_commit = None;
        self.selected_commit_files.clear();
        self.selected_commit_diff = None;
        self.dir_cache.clear();

        if is_repo {
            self.trigger_refresh();
            self.notify_success("Repository", "Repository loaded successfully");
        } else {
            self.notify_error("Repository", "Directory is not a Git repository");
        }
    }

    pub fn trigger_refresh(&mut self) {
        let Some(repo) = self.repo_path.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.is_busy = true;
        self.busy_message = "Refreshing repository...".to_string();

        thread::spawn(move || {
            let current_branch = git::get_current_branch(&repo).unwrap_or_else(|_| "HEAD".into());
            let branches = git::list_branches(&repo).unwrap_or_default();
            let remotes = git::list_remotes(&repo).unwrap_or_default();
            let status = git::get_status(&repo).unwrap_or_default();
            let commits = git::get_log(&repo, 50).unwrap_or_default();

            let _ = tx.send(AsyncMessage::RefreshDone {
                current_branch,
                branches,
                remotes,
                status,
                commits,
            });
        });
    }

    fn load_file_diff(&mut self, path: String, is_staged: bool, is_untracked: bool) {
        let Some(repo) = self.repo_path.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.selected_file = Some(SelectedFile {
            path: path.clone(),
            is_staged,
            is_untracked,
        });

        thread::spawn(move || {
            let raw_diff = if is_untracked {
                git::get_untracked_diff(&repo, &path).unwrap_or_default()
            } else {
                git::get_diff(&repo, Some(&path), is_staged).unwrap_or_default()
            };
            let diff = parse_git_diff(&raw_diff);
            let _ = tx.send(AsyncMessage::DiffLoaded {
                target: path,
                diff,
            });
        });
    }

    fn load_commit_details(&mut self, commit: CommitInfo) {
        let Some(repo) = self.repo_path.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let hash = commit.hash.clone();
        self.selected_commit = Some(commit);

        thread::spawn(move || {
            let files = git::get_commit_files(&repo, &hash).unwrap_or_default();
            let raw_diff = git::get_commit_diff(&repo, &hash).unwrap_or_default();
            let diff = parse_git_diff(&raw_diff);
            let _ = tx.send(AsyncMessage::CommitDiffLoaded {
                hash,
                files,
                diff,
            });
        });
    }

    fn load_file_content(&mut self, path: String) {
        let Some(repo) = self.repo_path.clone() else {
            return;
        };
        let tx = self.tx.clone();

        thread::spawn(move || {
            let content = git::read_file_content(&repo, &path)
                .unwrap_or_else(|e| format!("Could not read file: {}", e));
            let _ = tx.send(AsyncMessage::FileContentLoaded { path, content });
        });
    }

    fn run_async_op<F>(&mut self, action_name: &'static str, busy_msg: &str, f: F)
    where
        F: FnOnce(&Path) -> anyhow::Result<String> + Send + 'static,
    {
        let Some(repo) = self.repo_path.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.is_busy = true;
        self.busy_message = busy_msg.to_string();

        thread::spawn(move || {
            let res = f(&repo).map_err(|e| e.to_string());
            let _ = tx.send(AsyncMessage::OperationFinished {
                action_name: action_name.to_string(),
                result: res,
            });
        });
    }

    fn handle_async_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AsyncMessage::RefreshDone {
                    current_branch,
                    branches,
                    remotes,
                    status,
                    commits,
                } => {
                    self.current_branch = current_branch;
                    self.branches = branches;
                    self.remotes = remotes;
                    self.git_status = status;
                    self.commits = commits;
                    self.dir_cache.clear();
                    self.is_busy = false;

                    // If a file was selected, reload its diff
                    if let Some(sel) = self.selected_file.clone() {
                        self.load_file_diff(sel.path, sel.is_staged, sel.is_untracked);
                    }
                    ctx.request_repaint();
                }
                AsyncMessage::DiffLoaded { target, diff } => {
                    if let Some(sel) = &self.selected_file {
                        if sel.path == target {
                            self.current_diff = Some(diff);
                        }
                    }
                    ctx.request_repaint();
                }
                AsyncMessage::CommitDiffLoaded { hash, files, diff } => {
                    if let Some(sel) = &self.selected_commit {
                        if sel.hash == hash {
                            self.selected_commit_files = files;
                            self.selected_commit_diff = Some(diff);
                        }
                    }
                    ctx.request_repaint();
                }
                AsyncMessage::FileContentLoaded { path, content } => {
                    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                    let mut max_len = 0;
                    for l in &lines {
                        max_len = max_len.max(l.len());
                    }
                    self.file_view_path = Some(path);
                    self.file_view_content = Some(content);
                    self.file_view_lines = lines;
                    self.file_view_max_len = max_len;
                    ctx.request_repaint();
                }
                AsyncMessage::OperationFinished {
                    action_name,
                    result,
                } => {
                    self.is_busy = false;
                    match result {
                        Ok(msg) => {
                            let text = if msg.trim().is_empty() {
                                format!("{} completed successfully", action_name)
                            } else {
                                msg.trim().to_string()
                            };
                            self.notify_success(format!("{} Successful", action_name), text);
                        }
                        Err(err) => {
                            self.notify_error(format!("{} Failed", action_name), err);
                        }
                    }
                    self.dir_cache.clear();
                    self.trigger_refresh();
                    ctx.request_repaint();
                }
            }
        }
    }
}

impl eframe::App for RustyGitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_async_messages(&ctx);

        // Global Keyboard Shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.trigger_refresh();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                self.set_repo(folder);
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Num1)) {
            self.selected_tab = MainTab::Changes;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Num2)) {
            self.selected_tab = MainTab::Branches;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Num3)) {
            self.selected_tab = MainTab::History;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Num4)) {
            self.selected_tab = MainTab::Files;
        }

        // Top Navigation & Action Bar
        egui::Panel::top("top_bar")
            .exact_size(metrics::TOPBAR_H)
            .show_separator_line(true)
            .frame(
                egui::Frame::NONE
                    .fill(tokens::BG)
                    .inner_margin(egui::Margin::symmetric(metrics::SP_MD as i8, 8)),
            )
            .show(ui, |ui| {
                self.render_top_bar(ui);
            });

        // Bottom Status Bar
        egui::Panel::bottom("status_bar")
            .exact_size(metrics::STATUSBAR_H)
            .show_separator_line(true)
            .frame(
                egui::Frame::NONE
                    .fill(tokens::BG)
                    .inner_margin(egui::Margin::symmetric(metrics::SP_MD as i8, 4)),
            )
            .show(ui, |ui| {
                self.render_bottom_bar(ui);
            });

        // Left Navigation & Data Sidebar
        egui::Panel::left("left_sidebar")
            .resizable(true)
            .default_size(320.0)
            .min_size(240.0)
            .max_size(600.0)
            .show_separator_line(true)
            .frame(
                egui::Frame::NONE
                    .fill(tokens::BG)
                    .inner_margin(egui::Margin::symmetric(metrics::SP_SM as i8, metrics::SP_SM as i8)),
            )
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });

        // Main Center Content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(tokens::BG)
                    .inner_margin(egui::Margin::same(metrics::SP_MD as i8)),
            )
            .show(ui, |ui| {
                self.render_main_content(ui);
            });

        // Render Modals / Popups if open
        self.render_modals(&ctx);

        // Render floating Toast Alert if active
        self.render_toast_alert(&ctx);
    }
}

// UI Rendering Implementations
impl RustyGitApp {
    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(metrics::SP_SM, 0.0);

            // Brand: an amber-ruled mark with the branch glyph, "RustyGit" wordmark.
            let (mark, _mark_resp) = ui.allocate_exact_size(
                Vec2::splat(metrics::ICON_LOGO),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(mark, tokens::RADIUS_WIDGET, tokens::ACCENT_BG);
            icons::paint(
                ui.painter(),
                icons::glyph_in(mark, metrics::ICON_SIZE),
                icons::Icon::Branch,
                tokens::ACCENT,
            );
            ui.label(RichText::new("RustyGit").strong().heading().color(tokens::TEXT));

            // Open repository.
            if widgets::text_button(
                ui,
                Some(icons::Icon::Folder),
                "Open Repository",
                Btn::Soft,
                true,
            )
            .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.set_repo(folder);
                }
            }

            // Current repo path.
            if let Some(repo) = &self.repo_path {
                let (icon_rect, _) = ui.allocate_exact_size(
                    Vec2::splat(metrics::ICON_SIZE),
                    egui::Sense::hover(),
                );
                icons::paint(
                    ui.painter(),
                    icons::glyph_in(icon_rect, metrics::ICON_SIZE - 2.0),
                    icons::Icon::Folder,
                    tokens::TEXT_FAINT,
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(repo.to_string_lossy().to_string())
                            .monospace()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    )
                    .truncate(),
                );
            } else {
                ui.label(
                    RichText::new("No repository opened")
                        .weak()
                        .size(metrics::TEXT_SMALL)
                        .color(tokens::TEXT_FAINT),
                );
            }

            // Right-aligned actions: placed FIRST via right_to_left so they occupy the right edge
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(metrics::SP_XS, 0.0);

                // Busy indicator (right edge).
                if self.is_busy {
                    ui.spinner();
                    ui.label(
                        RichText::new(&self.busy_message)
                            .weak()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                }

                // Sync actions — the project's handlers, one button recipe.
                let enabled = self.is_git_repo && !self.is_busy;

                if widgets::text_button(ui, Some(icons::Icon::Refresh), "Refresh", Btn::Soft, enabled)
                    .clicked()
                {
                    self.trigger_refresh();
                }

                if widgets::text_button(ui, Some(icons::Icon::Push), "Push", Btn::Soft, enabled)
                    .clicked()
                {
                    self.run_async_op("Push", "Pushing to remote...", |dir| git::push(dir));
                }

                if widgets::text_button(ui, Some(icons::Icon::Pull), "Pull", Btn::Soft, enabled)
                    .clicked()
                {
                    self.run_async_op("Pull", "Pulling from remote...", |dir| git::pull(dir));
                }

                if widgets::text_button(ui, Some(icons::Icon::Fetch), "Fetch", Btn::Soft, enabled)
                    .clicked()
                {
                    self.run_async_op("Fetch", "Fetching remote refs...", |dir| git::fetch(dir));
                }

                if widgets::text_button(ui, Some(icons::Icon::Link), "Remote", Btn::Soft, enabled)
                    .clicked()
                {
                    self.show_connect_remote_modal = true;
                    if self.remote_name_input.trim().is_empty() {
                        self.remote_name_input = "origin".to_string();
                    }
                }

                if widgets::text_button(ui, Some(icons::Icon::File), ".gitignore", Btn::Soft, enabled)
                    .clicked()
                {
                    self.open_gitignore_modal();
                }

                // Current branch chip: opens the Branches view (matching CTRL_H height).
                if self.is_git_repo && !self.current_branch.is_empty() {
                    let name = self.current_branch.clone();
                    let font = egui::FontId::proportional(metrics::TEXT_BODY);
                    let text_w = ui
                        .painter()
                        .layout_no_wrap(name.clone(), font.clone(), tokens::ACCENT)
                        .size()
                        .x;
                    let w = text_w + metrics::ICON_SIZE + 20.0;
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(w, metrics::CTRL_H),
                        egui::Sense::click(),
                    );
                    let p = &ui.painter_at(rect);
                    p.rect_filled(
                        rect,
                        tokens::RADIUS_WIDGET,
                        if resp.hovered() { tokens::SURFACE_HI } else { tokens::ACCENT_BG },
                    );
                    p.rect_stroke(
                        rect,
                        tokens::RADIUS_WIDGET,
                        egui::Stroke::new(1.0, if resp.hovered() { tokens::ACCENT } else { tokens::ACCENT_BG }),
                        egui::StrokeKind::Middle,
                    );
                    if resp.has_focus() {
                        p.rect_stroke(
                            rect.expand(1.5),
                            tokens::RADIUS_WIDGET,
                            egui::Stroke::new(1.5, tokens::ACCENT),
                            egui::StrokeKind::Middle,
                        );
                    }
                    icons::paint(
                        p,
                        egui::Rect::from_center_size(
                            egui::Pos2::new(rect.left() + 8.0 + metrics::ICON_SIZE * 0.5, rect.center().y),
                            Vec2::splat(metrics::ICON_SIZE),
                        ),
                        icons::Icon::Branch,
                        tokens::ACCENT,
                    );
                    widgets::paint_text(
                        p,
                        egui::Pos2::new(rect.left() + 8.0 + metrics::ICON_SIZE + 6.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &name,
                        metrics::TEXT_BODY,
                        tokens::ACCENT,
                        false,
                    );
                    if resp.on_hover_text("Open branches").clicked() {
                        self.selected_tab = MainTab::Branches;
                    }
                }
            });
        });
    }

    fn render_bottom_bar(&mut self, ui: &mut egui::Ui) {
        // Expired notification is cleared so the status line returns to "Ready".
        if let Some((_, _, created_at)) = &self.notification {
            if created_at.elapsed().as_secs() >= 6 {
                self.notification = None;
            }
        }

        ui.horizontal_centered(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(_repo) = &self.repo_path {
                    if self.is_git_repo {
                        let total_changed = self.git_status.staged.len()
                            + self.git_status.unstaged.len()
                            + self.git_status.untracked.len();
                        ui.label(
                            RichText::new(format!("Changes: {} files", total_changed))
                                .size(metrics::TEXT_SMALL)
                                .color(tokens::TEXT_FAINT),
                        );
                    } else {
                        ui.label(
                            RichText::new("Not a Git repository")
                                .color(tokens::ACCENT)
                                .size(metrics::TEXT_SMALL),
                        );
                    }
                }
            });

            if let Some((msg, is_error, _created_at)) = &self.notification {
                let (icon, color) = if *is_error {
                    (icons::Icon::Warn, tokens::DANGER_TEXT)
                } else {
                    (icons::Icon::Check, tokens::SUCCESS)
                };
                let (icon_rect, _) = ui.allocate_exact_size(
                    Vec2::splat(metrics::ICON_SIZE),
                    egui::Sense::hover(),
                );
                icons::paint(
                    ui.painter(),
                    icons::glyph_in(icon_rect, metrics::ICON_SIZE - 1.0),
                    icon,
                    color,
                );
                ui.label(
                    RichText::new(msg.clone())
                        .size(metrics::TEXT_SMALL)
                        .color(tokens::TEXT_MUTED),
                );
            } else {
                ui.label(
                    RichText::new("Ready")
                        .weak()
                        .size(metrics::TEXT_SMALL)
                        .color(tokens::TEXT_FAINT),
                );
            }
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        if !self.is_git_repo {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics::SP_LG);
                let (icon_rect, _) = ui.allocate_exact_size(
                    Vec2::splat(metrics::ICON_LOGO * 2.0),
                    egui::Sense::hover(),
                );
                icons::paint(
                    ui.painter(),
                    icons::glyph_in(icon_rect, metrics::ICON_LOGO * 1.6),
                    icons::Icon::Branch,
                    tokens::TEXT_FAINT,
                );
                ui.add_space(metrics::SP_SM);
                ui.label(
                    RichText::new("Not in a Git repository")
                        .strong()
                        .size(metrics::TEXT_BODY)
                        .color(tokens::TEXT),
                );
                ui.add_space(metrics::SP_SM);
                if widgets::text_button(
                    ui,
                    Some(icons::Icon::Branch),
                    "Initialize Git Here",
                    Btn::Primary,
                    true,
                )
                .clicked()
                {
                    if let Some(dir) = self.repo_path.clone() {
                        if let Err(e) = git::init_repo(&dir) {
                            self.notify_error("Init", format!("Failed to init: {}", e));
                        } else {
                            self.notify_success("Init", "Initialized Git repository!");
                            self.set_repo(dir);
                        }
                    }
                }
            });
            return;
        }

        ui.add_space(metrics::SP_XS);

        // Tab rail.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            let changes_badge = self.git_status.staged.len()
                + self.git_status.unstaged.len()
                + self.git_status.untracked.len();
            let changes_label = if changes_badge > 0 {
                format!("Changes ({})", changes_badge)
            } else {
                "Changes".into()
            };

            self.tab_rail(ui, &changes_label, MainTab::Changes);
            self.tab_rail(ui, "Branches", MainTab::Branches);
            self.tab_rail(ui, "History", MainTab::History);
            self.tab_rail(ui, "Files", MainTab::Files);
        });

        ui.add_space(metrics::SP_SM);
        ui.separator();
        ui.add_space(metrics::SP_SM);

        match self.selected_tab {
            MainTab::Changes => self.render_changes_tab(ui),
            MainTab::Branches => self.render_branches_tab(ui),
            MainTab::History => self.render_history_tab(ui),
            MainTab::Files => self.render_files_tab(ui),
        }
    }

    fn tab_rail(&mut self, ui: &mut egui::Ui, label: &str, tab: MainTab) {
        let selected = self.selected_tab == tab;
        let font = egui::FontId::proportional(metrics::TEXT_BODY);
        let text_color = if selected {
            tokens::TEXT
        } else {
            tokens::TEXT_MUTED
        };
        let gal = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), text_color);
        let w = gal.size().x + metrics::SP_MD + metrics::SP_SM;
        let h = metrics::CTRL_H_SM + 4.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());

        let p = ui.painter();
        if selected {
            p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::ACCENT_BG);
            p.rect_stroke(
                rect.shrink(0.5),
                tokens::RADIUS_CHIP,
                egui::Stroke::new(1.0, tokens::ACCENT),
                egui::StrokeKind::Inside,
            );
        } else if resp.hovered() {
            p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::SURFACE_HI);
            p.rect_stroke(
                rect.shrink(0.5),
                tokens::RADIUS_CHIP,
                egui::Stroke::new(1.0, tokens::HAIRLINE),
                egui::StrokeKind::Inside,
            );
        }
        if resp.has_focus() {
            p.rect_stroke(
                rect.shrink(0.5),
                tokens::RADIUS_CHIP,
                egui::Stroke::new(1.5, tokens::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected {
            tokens::ACCENT
        } else if resp.hovered() {
            tokens::TEXT
        } else {
            tokens::TEXT_MUTED
        };
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font,
            color,
        );
        if resp.clicked() {
            self.selected_tab = tab;
        }
    }

    fn render_changes_tab(&mut self, ui: &mut egui::Ui) {
        let repo = self.repo_path.clone();

        let mut staged_to_unstage = None;
        let mut staged_to_load = None;
        let mut unstaged_to_stage: Option<String> = None;
        let mut unstaged_to_discard: Option<(String, bool)> = None;
        let mut unstaged_to_load: Option<(String, bool, bool)> = None;
        let mut untracked_to_ignore: Option<String> = None;

        let avail_h = ui.available_height();
        let composer_h = metrics::COMPOSER_H;
        let list_h = (avail_h - composer_h - metrics::SP_SM).max(60.0);

        let scroll_resp = egui::ScrollArea::vertical()
            .id_salt("changes_scroll")
            .auto_shrink([false, false])
            .max_height(list_h)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                // Staged section.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Staged Changes ({})", self.git_status.staged.len()))
                            .strong()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if widgets::text_button(
                            ui,
                            Some(icons::Icon::Minus),
                            "Unstage All",
                            Btn::Quiet,
                            !self.git_status.staged.is_empty(),
                        )
                        .clicked()
                        {
                            if let Some(r) = &repo {
                                if let Err(e) = git::unstage_all(r) {
                                    self.notify_error("Unstage All", format!("Failed to unstage all: {}", e));
                                } else {
                                    self.notify_success("Unstage All", "Unstaged all changes");
                                    if let Some(sel) = &mut self.selected_file {
                                        if sel.is_staged {
                                            sel.is_staged = false;
                                        }
                                    }
                                }
                                self.trigger_refresh();
                            }
                        }
                    });
                });

                for file in &self.git_status.staged {
                    let is_selected = self
                        .selected_file
                        .as_ref()
                        .map(|s| s.path == file.path && s.is_staged)
                        .unwrap_or(false);
                    let row_id = ui.scope_id().with(("staged_row", &file.path));
                    let (clicked, action) = file_row(
                        ui,
                        row_id,
                        &file.path,
                        file.index_status,
                        is_selected,
                        &[
                            RowBtn {
                                icon: icons::Icon::Minus,
                                action: RowAction::Unstage,
                                hover: "Unstage file",
                            },
                        ],
                    );
                    if action == Some(RowAction::Unstage) {
                        staged_to_unstage = Some(file.path.clone());
                    } else if clicked {
                        staged_to_load = Some(file.path.clone());
                    }
                }
                if self.git_status.staged.is_empty() {
                    ui.label(
                        RichText::new("No files staged")
                            .weak()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_FAINT),
                    );
                }

                ui.add_space(metrics::SP_SM);

                // Unstaged section (incl. untracked below).
                let unstaged_total =
                    self.git_status.unstaged.len() + self.git_status.untracked.len();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Unstaged Changes ({})", unstaged_total))
                            .strong()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if widgets::text_button(
                            ui,
                            Some(icons::Icon::Plus),
                            "Stage All",
                            Btn::Quiet,
                            unstaged_total > 0,
                        )
                        .clicked()
                        {
                            if let Some(r) = &repo {
                                if let Err(e) = git::stage_all(r) {
                                    self.notify_error("Stage All", format!("Failed to stage all: {}", e));
                                } else {
                                    self.notify_success("Stage All", "Staged all changes");
                                    if let Some(sel) = &mut self.selected_file {
                                        if !sel.is_staged {
                                            sel.is_staged = true;
                                            sel.is_untracked = false;
                                        }
                                    }
                                }
                                self.trigger_refresh();
                            }
                        }
                    });
                });

                for file in &self.git_status.unstaged {
                    let is_selected = self
                        .selected_file
                        .as_ref()
                        .map(|s| s.path == file.path && !s.is_staged)
                        .unwrap_or(false);
                    let row_id = ui.scope_id().with(("unstaged_row", &file.path));
                    let (clicked, action) = file_row(
                        ui,
                        row_id,
                        &file.path,
                        file.worktree_status,
                        is_selected,
                        &[
                            RowBtn {
                                icon: icons::Icon::Plus,
                                action: RowAction::Stage,
                                hover: "Stage file",
                            },
                            RowBtn {
                                icon: icons::Icon::X,
                                action: RowAction::Discard,
                                hover: "Discard changes",
                            },
                        ],
                    );
                    match action {
                        Some(RowAction::Stage) => unstaged_to_stage = Some(file.path.clone()),
                        Some(RowAction::Discard) => {
                            unstaged_to_discard = Some((file.path.clone(), false));
                        }
                        _ if clicked => {
                            unstaged_to_load = Some((file.path.clone(), false, false));
                        }
                        _ => {}
                    }
                }

                for file in &self.git_status.untracked {
                    let is_selected = self
                        .selected_file
                        .as_ref()
                        .map(|s| s.path == file.path && !s.is_staged)
                        .unwrap_or(false);
                    let row_id = ui.scope_id().with(("untracked_row", &file.path));
                    let (clicked, action) = file_row(
                        ui,
                        row_id,
                        &file.path,
                        '?',
                        is_selected,
                        &[
                            RowBtn {
                                icon: icons::Icon::Plus,
                                action: RowAction::Stage,
                                hover: "Stage file",
                            },
                            RowBtn {
                                icon: icons::Icon::File,
                                action: RowAction::Ignore,
                                hover: "Add to .gitignore",
                            },
                            RowBtn {
                                icon: icons::Icon::X,
                                action: RowAction::Discard,
                                hover: "Delete untracked file",
                            },
                        ],
                    );
                    match action {
                        Some(RowAction::Stage) => unstaged_to_stage = Some(file.path.clone()),
                        Some(RowAction::Ignore) => {
                            untracked_to_ignore = Some(file.path.clone());
                        }
                        Some(RowAction::Discard) => {
                            unstaged_to_discard = Some((file.path.clone(), true));
                        }
                        _ if clicked => {
                            unstaged_to_load = Some((file.path.clone(), false, true));
                        }
                        _ => {}
                    }
                }

                if unstaged_total == 0 {
                    ui.label(
                        RichText::new("Clean working directory")
                            .weak()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_FAINT),
                    );
                }
            });

        // Handlers (unchanged wiring), after the borrow of the list is over.
        if let Some((p, is_staged, is_untracked)) = unstaged_to_load {
            self.load_file_diff(p, is_staged, is_untracked);
        }
        if let Some(p) = unstaged_to_stage {
            if let Some(r) = &repo {
                if let Err(e) = git::stage_files(r, &[p.clone()]) {
                    self.notify_error("Stage File", format!("Failed to stage {}: {}", p, e));
                } else {
                    self.notify_success("Stage File", format!("Staged {}", p));
                    if let Some(sel) = &mut self.selected_file {
                        if sel.path == p && !sel.is_staged {
                            sel.is_staged = true;
                            sel.is_untracked = false;
                        }
                    }
                }
                self.trigger_refresh();
            }
        }
        if let Some(d) = unstaged_to_discard {
            self.show_discard_dialog = Some(d);
        }
        if let Some(p) = untracked_to_ignore {
            self.open_gitignore_modal_with_pattern(&p);
        }
        if let Some(p) = staged_to_load {
            self.load_file_diff(p, true, false);
        }
        if let Some(p) = staged_to_unstage {
            if let Some(r) = &repo {
                if let Err(e) = git::unstage_files(r, &[p.clone()]) {
                    self.notify_error("Unstage File", format!("Failed to unstage {}: {}", p, e));
                } else {
                    self.notify_success("Unstage File", format!("Unstaged {}", p));
                    if let Some(sel) = &mut self.selected_file {
                        if sel.path == p && sel.is_staged {
                            sel.is_staged = false;
                        }
                    }
                }
                self.trigger_refresh();
            }
        }

        // Keep composer docked at the bottom of the sidebar
        let used_h = scroll_resp.inner_rect.height();
        if used_h < list_h {
            ui.add_space(list_h - used_h);
        }
        ui.add_space(metrics::SP_SM);

        // Commit message composer
        egui::Frame::NONE
            .fill(tokens::SURFACE)
            .corner_radius(tokens::RADIUS_WIDGET)
            .inner_margin(egui::Margin::same(metrics::SP_SM as i8))
            .stroke(egui::Stroke::new(1.0, tokens::HAIRLINE))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Commit Message")
                            .strong()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    let text_edit = egui::TextEdit::multiline(&mut self.commit_message)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter commit message... (Ctrl+Enter to commit)");
                    let response = ui.add(text_edit);

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.commit_amend, "Amend");

                        let can_commit =
                            (!self.commit_message.trim().is_empty() || self.commit_amend)
                                && (!self.git_status.staged.is_empty() || self.commit_amend);

                        let commit_shortcut = response
                            .ctx
                            .input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if widgets::text_button(
                                    ui,
                                    Some(icons::Icon::Check),
                                    "Commit",
                                    Btn::Primary,
                                    can_commit,
                                )
                                .clicked()
                                    || (can_commit && commit_shortcut)
                                {
                                    let msg = self.commit_message.clone();
                                    let amend = self.commit_amend;
                                    self.commit_message.clear();
                                    self.commit_amend = false;
                                    self.run_async_op("Commit", "Committing changes...", move |dir| {
                                        git::commit(dir, &msg, amend)?;
                                        Ok("Commit created".into())
                                    });
                                }
                            },
                        );
                    });
                });
            });
    }

    fn render_branches_tab(&mut self, ui: &mut egui::Ui) {
        // Current branch card.
        if !self.current_branch.is_empty() {
            egui::Frame::NONE
                .fill(tokens::SURFACE)
                .corner_radius(tokens::RADIUS_WIDGET)
                .inner_margin(egui::Margin::same(metrics::SP_SM as i8))
                .stroke(egui::Stroke::new(1.0, tokens::HAIRLINE))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let (icon_rect, _) = ui.allocate_exact_size(
                                Vec2::splat(metrics::ICON_SIZE),
                                egui::Sense::hover(),
                            );
                            icons::paint(
                                ui.painter(),
                                icons::glyph_in(icon_rect, metrics::ICON_SIZE),
                                icons::Icon::Branch,
                                tokens::ACCENT,
                            );
                            ui.label(
                                RichText::new(&self.current_branch)
                                    .strong()
                                    .size(metrics::TEXT_BODY)
                                    .color(tokens::TEXT),
                            );
                        });
                        if let Some(cur) = self.branches.iter().find(|b| b.is_current) {
                            if let Some(up) = &cur.upstream {
                                ui.label(
                                    RichText::new(format!("Tracks: {}", up))
                                        .weak()
                                        .size(metrics::TEXT_SMALL)
                                        .color(tokens::TEXT_FAINT),
                                );
                            }
                        }
                    });
                });
            ui.add_space(metrics::SP_SM);
        }

        if widgets::text_button(ui, Some(icons::Icon::Plus), "New Branch", Btn::Soft, true).clicked()
        {
            self.show_new_branch_modal = true;
            self.new_branch_name.clear();
        }

        ui.add_space(metrics::SP_SM);
        let local_count = self.branches.iter().filter(|b| !b.is_remote).count();
        let remote_count = self.branches.iter().filter(|b| b.is_remote).count();
        ui.label(
            RichText::new(format!("Local branches: {}", local_count))
                .size(metrics::TEXT_SMALL)
                .color(tokens::TEXT_MUTED),
        );
        ui.label(
            RichText::new(format!("Remote branches: {}", remote_count))
                .size(metrics::TEXT_SMALL)
                .color(tokens::TEXT_MUTED),
        );
        ui.label(
            RichText::new(format!("Connected remotes: {}", self.remotes.len()))
                .size(metrics::TEXT_SMALL)
                .color(tokens::TEXT_MUTED),
        );
        ui.add_space(metrics::SP_SM);
        if widgets::text_button(ui, Some(icons::Icon::Link), "Connect Remote", Btn::Soft, true).clicked() {
            self.show_connect_remote_modal = true;
            if self.remote_name_input.trim().is_empty() {
                self.remote_name_input = "origin".to_string();
            }
        }
        ui.add_space(metrics::SP_XS);
        ui.label(
            RichText::new("Manage and switch branches in the central view.")
                .weak()
                .size(metrics::TEXT_SMALL)
                .color(tokens::TEXT_FAINT),
        );
    }

    fn render_history_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Commit History")
                .strong()
                .size(metrics::TEXT_BODY)
                .color(tokens::TEXT),
        );
        ui.add_space(metrics::SP_2XS);

        ui.add(
            egui::TextEdit::singleline(&mut self.search_query)
                .desired_width(f32::INFINITY)
                .hint_text("Filter commits..."),
        );
        ui.add_space(metrics::SP_XS);

        let query = self.search_query.to_lowercase();
        let commits: Vec<CommitInfo> = self
            .commits
            .iter()
            .filter(|c| {
                query.is_empty()
                    || c.summary.to_lowercase().contains(&query)
                    || c.author_name.to_lowercase().contains(&query)
                    || c.short_hash.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        let mut commit_to_select = None;

        egui::ScrollArea::vertical()
            .id_salt("history_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                if commits.is_empty() {
                    ui.label(
                        RichText::new("No commits found")
                            .weak()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_FAINT),
                    );
                } else {
                    for c in commits {
                        let is_selected = self
                            .selected_commit
                            .as_ref()
                            .map(|s| s.hash == c.hash)
                            .unwrap_or(false);
                        let row_id = ui.scope_id().with(("history_row", &c.hash));
                        if commit_row(ui, row_id, &c, is_selected) {
                            commit_to_select = Some(c.clone());
                        }
                    }
                }
            });

        if let Some(c) = commit_to_select {
            self.load_commit_details(c);
        }
    }

    fn render_files_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Repository Files")
                .strong()
                .size(metrics::TEXT_BODY)
                .color(tokens::TEXT),
        );
        ui.add_space(metrics::SP_2XS);

        let Some(repo) = self.repo_path.clone() else {
            return;
        };

        let mut file_to_read = None;

        egui::ScrollArea::vertical()
            .id_salt("files_tree_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                render_dir_tree(ui, &repo, &repo, &mut self.dir_cache, &mut file_to_read);
            });

        if let Some(p) = file_to_read {
            self.load_file_content(p);
        }
    }

    fn render_main_content(&mut self, ui: &mut egui::Ui) {
        match self.selected_tab {
            MainTab::Changes => {
                if let Some(sel) = &self.selected_file {
                    let mut stage_toggle = None;
                    let mut ignore_pattern_to_open = None;
                    ui.horizontal(|ui| {
                        // 1. Right-side diff statistics first
                        if let Some(diff) = &self.current_diff {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("-{}", diff.deletions))
                                            .monospace()
                                            .strong()
                                            .color(tokens::DANGER_TEXT),
                                    );
                                    ui.label(
                                        RichText::new(format!("+{}", diff.additions))
                                            .monospace()
                                            .strong()
                                            .color(tokens::SUCCESS),
                                    );
                                },
                            );
                        }

                        // 2. Status pill and file path
                        let (label, fg, bg) = if sel.is_staged {
                            ("Staged", tokens::SUCCESS, tokens::ADD_BG)
                        } else if sel.is_untracked {
                            ("Untracked", tokens::INFO, tokens::INFO_BG)
                        } else {
                            ("Unstaged", tokens::ACCENT, tokens::ACCENT_BG)
                        };
                        widgets::chip(ui, label, fg, bg, false);

                        let is_staged = sel.is_staged;
                        let sel_path = sel.path.clone();
                        if is_staged {
                            if widgets::text_button(ui, Some(icons::Icon::Minus), "Unstage", Btn::Soft, true).clicked() {
                                stage_toggle = Some((sel_path.clone(), false));
                            }
                        } else {
                            if widgets::text_button(ui, Some(icons::Icon::Plus), "Stage", Btn::Soft, true).clicked() {
                                stage_toggle = Some((sel_path.clone(), true));
                            }
                            if sel.is_untracked {
                                if widgets::text_button(ui, Some(icons::Icon::File), "Ignore...", Btn::Quiet, true).clicked() {
                                    ignore_pattern_to_open = Some(sel_path.clone());
                                }
                            }
                        }

                        ui.add(
                            egui::Label::new(
                                RichText::new(&sel.path)
                                    .strong()
                                    .heading()
                                    .color(tokens::TEXT),
                            )
                            .truncate(),
                        );
                    });

                    if let Some((p, to_stage)) = stage_toggle {
                        if let Some(r) = self.repo_path.clone() {
                            if to_stage {
                                if let Err(e) = git::stage_files(&r, &[p.clone()]) {
                                    self.notify_error("Stage File", format!("Failed to stage {}: {}", p, e));
                                } else {
                                    self.notify_success("Stage File", format!("Staged {}", p));
                                    if let Some(s) = &mut self.selected_file {
                                        s.is_staged = true;
                                        s.is_untracked = false;
                                    }
                                }
                            } else {
                                if let Err(e) = git::unstage_files(&r, &[p.clone()]) {
                                    self.notify_error("Unstage File", format!("Failed to unstage {}: {}", p, e));
                                } else {
                                    self.notify_success("Unstage File", format!("Unstaged {}", p));
                                    if let Some(s) = &mut self.selected_file {
                                        s.is_staged = false;
                                    }
                                }
                            }
                            self.trigger_refresh();
                        }
                    }
                    if let Some(p) = ignore_pattern_to_open {
                        self.open_gitignore_modal_with_pattern(&p);
                    }
                    ui.add_space(metrics::SP_2XS);
                    ui.separator();
                    ui.add_space(metrics::SP_2XS);

                    if let Some(diff) = &self.current_diff {
                        egui::Frame::NONE
                            .fill(tokens::WELL)
                            .corner_radius(tokens::RADIUS_WIDGET)
                            .inner_margin(egui::Margin::same(0))
                            .show(ui, |ui| {
                                render_diff_viewer(ui, diff);
                            });
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.spinner();
                        });
                    }
                } else {
                    empty_state(
                        ui,
                        icons::Icon::File,
                        "Select a changed file from the sidebar to view diff",
                    );
                }
            }
            MainTab::History => {
                if let Some(c) = self.selected_commit.clone() {
                    egui::Frame::NONE
                        .fill(tokens::SURFACE)
                        .corner_radius(tokens::RADIUS_WIDGET)
                        .inner_margin(egui::Margin::same(metrics::SP_SM as i8))
                        .stroke(egui::Stroke::new(1.0, tokens::HAIRLINE))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        widgets::chip(
                                            ui,
                                            &c.short_hash,
                                            tokens::TEXT_MUTED,
                                            tokens::META_BG,
                                            true,
                                        );
                                    },
                                );

                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&c.summary)
                                            .strong()
                                            .heading()
                                            .color(tokens::TEXT),
                                    )
                                    .truncate(),
                                );
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Author: {} <{}>",
                                        c.author_name, c.author_email
                                    ))
                                    .weak()
                                    .size(metrics::TEXT_SMALL)
                                    .color(tokens::TEXT_MUTED),
                                );
                                ui.label(
                                    RichText::new(format!("Date: {}", c.date))
                                        .weak()
                                        .size(metrics::TEXT_SMALL)
                                        .color(tokens::TEXT_MUTED),
                                );
                            });

                            if !c.body.is_empty() {
                                ui.add_space(metrics::SP_2XS);
                                ui.label(
                                    RichText::new(&c.body)
                                        .size(metrics::TEXT_BODY)
                                        .color(tokens::TEXT),
                                );
                            }

                            ui.add_space(metrics::SP_2XS);
                            ui.label(
                                RichText::new(format!("Commit: {}", c.hash))
                                    .monospace()
                                    .weak()
                                    .size(metrics::TEXT_SMALL)
                                    .color(tokens::TEXT_FAINT),
                            );
                        });

                    ui.add_space(metrics::SP_XS);
                    ui.separator();
                    ui.add_space(metrics::SP_2XS);

                    if !self.selected_commit_files.is_empty() {
                        ui.collapsing(
                            RichText::new(format!(
                                "Changed files ({})",
                                self.selected_commit_files.len()
                            ))
                            .color(tokens::TEXT_MUTED),
                            |ui| {
                                for f in &self.selected_commit_files {
                                    ui.horizontal(|ui| {
                                        let (icon_rect, _) = ui.allocate_exact_size(
                                            Vec2::splat(metrics::ICON_SIZE - 2.0),
                                            egui::Sense::hover(),
                                        );
                                        icons::paint(
                                            ui.painter(),
                                            icons::glyph_in(
                                                icon_rect,
                                                metrics::ICON_SIZE - 4.0,
                                            ),
                                            icons::Icon::File,
                                            tokens::TEXT_FAINT,
                                        );
                                        ui.label(
                                            RichText::new(f.clone())
                                                .monospace()
                                                .size(metrics::TEXT_SMALL)
                                                .color(tokens::TEXT_MUTED),
                                        );
                                    });
                                }
                            },
                        );
                        ui.add_space(metrics::SP_2XS);
                    }

                    if let Some(diff) = &self.selected_commit_diff {
                        egui::Frame::NONE
                            .fill(tokens::WELL)
                            .corner_radius(tokens::RADIUS_WIDGET)
                            .inner_margin(egui::Margin::same(0))
                            .show(ui, |ui| {
                                render_diff_viewer(ui, diff);
                            });
                    } else {
                        ui.spinner();
                    }
                } else {
                    empty_state(
                        ui,
                        icons::Icon::Branch,
                        "Select a commit from the history list to view details and diff",
                    );
                }
            }
            MainTab::Branches => {
                // The live branch lists — wide rows in center panel
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Branch Manager")
                            .strong()
                            .heading()
                            .color(tokens::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::text_button(ui, Some(icons::Icon::Plus), "New Branch", Btn::Primary, true).clicked()
                        {
                            self.show_new_branch_modal = true;
                            self.new_branch_name.clear();
                        }
                    });
                });
                ui.add_space(metrics::SP_2XS);
                ui.separator();
                ui.add_space(metrics::SP_SM);

                let mut branch_to_checkout = None;
                let mut branch_to_delete = None;
                let mut remote_to_remove = None;

                egui::ScrollArea::vertical()
                    .id_salt("branches_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                        ui.label(
                            RichText::new("Local Branches")
                                .strong()
                                .size(metrics::TEXT_SMALL)
                                .color(tokens::TEXT_MUTED),
                        );
                        for b in self.branches.iter().filter(|b| !b.is_remote) {
                            let row_id = ui.scope_id().with(("local_branch", &b.name));
                            let (checkout, delete) = branch_row(
                                ui,
                                row_id,
                                &b.name,
                                b.upstream.as_deref(),
                                b.is_current,
                                false,
                            );
                            if let Some(name) = checkout {
                                branch_to_checkout = Some(name);
                            }
                            if let Some(name) = delete {
                                branch_to_delete = Some(name);
                            }
                        }

                        ui.add_space(metrics::SP_SM);
                        ui.label(
                            RichText::new("Remote Branches")
                                .strong()
                                .size(metrics::TEXT_SMALL)
                                .color(tokens::TEXT_MUTED),
                        );
                        for b in self.branches.iter().filter(|b| b.is_remote) {
                            let row_id = ui.scope_id().with(("remote_branch", &b.name));
                            let (checkout, delete) = branch_row(
                                ui,
                                row_id,
                                &b.name,
                                b.upstream.as_deref(),
                                false,
                                true,
                            );
                            if let Some(name) = checkout {
                                branch_to_checkout = Some(name);
                            }
                            if let Some(name) = delete {
                                branch_to_delete = Some(name);
                            }
                        }

                        ui.add_space(metrics::SP_MD);
                        ui.horizontal(|ui| {
                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(metrics::ICON_SIZE), egui::Sense::hover());
                            icons::paint(ui.painter(), icons::glyph_in(icon_rect, metrics::ICON_SIZE - 2.0), icons::Icon::Link, tokens::ACCENT);
                            ui.label(
                                RichText::new("Connected Remotes")
                                    .strong()
                                    .size(metrics::TEXT_SMALL)
                                    .color(tokens::TEXT_MUTED),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::text_button(ui, Some(icons::Icon::Plus), "Add Remote", Btn::Soft, true).clicked() {
                                    self.show_connect_remote_modal = true;
                                    if self.remote_name_input.trim().is_empty() {
                                        self.remote_name_input = "origin".to_string();
                                    }
                                }
                            });
                        });

                        if self.remotes.is_empty() {
                            ui.label(
                                RichText::new("No remotes configured. Click 'Add Remote' or 'Remote' in top bar to connect to GitHub, GitLab, etc.")
                                    .weak()
                                    .size(metrics::TEXT_SMALL)
                                    .color(tokens::TEXT_FAINT),
                            );
                        } else {
                            for r in &self.remotes {
                                egui::Frame::NONE
                                    .fill(tokens::SURFACE)
                                    .corner_radius(tokens::RADIUS_WIDGET)
                                    .stroke(egui::Stroke::new(1.0, tokens::HAIRLINE))
                                    .inner_margin(egui::Margin::symmetric(metrics::SP_SM as i8, 6))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(metrics::ICON_SIZE), egui::Sense::hover());
                                            icons::paint(ui.painter(), icons::glyph_in(icon_rect, metrics::ICON_SIZE - 2.0), icons::Icon::Link, tokens::ACCENT);
                                            ui.label(RichText::new(&r.name).strong().color(tokens::TEXT));
                                            ui.label(RichText::new(&r.url).monospace().size(metrics::TEXT_SMALL).color(tokens::TEXT_MUTED));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if widgets::text_button(ui, None, "Remove", Btn::Quiet, true).clicked() {
                                                    remote_to_remove = Some(r.name.clone());
                                                }
                                            });
                                        });
                                    });
                            }
                        }
                    });

                if let Some(r_name) = remote_to_remove {
                    self.run_async_op("Remove Remote", "Removing remote...", move |dir| {
                        git::remove_remote(dir, &r_name)?;
                        Ok(format!("Removed remote '{}'", r_name))
                    });
                }

                if let Some(b) = branch_to_checkout {
                    self.run_async_op("Checkout", "Checking out branch...", move |dir| {
                        git::checkout_branch(dir, &b)?;
                        Ok(format!("Switched to branch '{}'", b))
                    });
                }

                if let Some(b) = branch_to_delete {
                    self.show_delete_branch_dialog = Some(b);
                }
            }
            MainTab::Files => {
                if let (Some(path), Some(content)) =
                    (&self.file_view_path.clone(), &self.file_view_content.clone())
                {
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(format!("{} lines", content.lines().count()))
                                        .weak()
                                        .size(metrics::TEXT_SMALL)
                                        .color(tokens::TEXT_FAINT),
                                );
                            },
                        );

                        ui.add(
                            egui::Label::new(
                                RichText::new(path.clone())
                                    .heading()
                                    .strong()
                                    .color(tokens::TEXT),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(metrics::SP_2XS);
                    ui.separator();
                    ui.add_space(metrics::SP_2XS);

                    egui::Frame::NONE
                        .fill(tokens::WELL)
                        .corner_radius(tokens::RADIUS_WIDGET)
                        .inner_margin(egui::Margin::same(0))
                        .show(ui, |ui| {
                            render_code_viewer(ui, &self.file_view_lines, self.file_view_max_len);
                        });
                } else {
                    empty_state(
                        ui,
                        icons::Icon::File,
                        "Select a file from the repository tree to view code",
                    );
                }
            }
        }
    }

    fn render_modals(&mut self, ctx: &egui::Context) {
        let window_frame = egui::Frame::NONE
            .fill(tokens::SURFACE)
            .stroke(egui::Stroke::new(1.0, tokens::HAIRLINE))
            .corner_radius(tokens::RADIUS_WINDOW)
            .inner_margin(egui::Margin::same(metrics::SP_MD as i8));

        // Modal: Create New Branch
        if self.show_new_branch_modal {
            egui::Window::new(egui::WidgetText::from("Create New Branch"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(window_frame)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("Enter new branch name:")
                            .size(metrics::TEXT_BODY)
                            .color(tokens::TEXT),
                    );
                    ui.add_space(metrics::SP_2XS);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_branch_name)
                            .desired_width(260.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.checkbox(&mut self.new_branch_checkout, "Checkout branch immediately");

                    ui.add_space(metrics::SP_SM);
                    ui.horizontal(|ui| {
                        let can_create = !self.new_branch_name.trim().is_empty();
                        if widgets::text_button(
                            ui,
                            None,
                            "Create",
                            Btn::Primary,
                            can_create,
                        )
                        .clicked()
                        {
                            let name = self.new_branch_name.trim().to_string();
                            let checkout = self.new_branch_checkout;
                            self.show_new_branch_modal = false;
                            self.run_async_op("Create Branch", "Creating branch...", move |dir| {
                                git::create_branch(dir, &name, checkout)?;
                                Ok(format!("Created branch '{}'", name))
                            });
                        }
                        if widgets::text_button(ui, None, "Cancel", Btn::Soft, true).clicked() {
                            self.show_new_branch_modal = false;
                        }
                    });
                });
        }

        // Modal: Confirm Discard Changes
        if let Some((path, is_untracked)) = self.show_discard_dialog.clone() {
            egui::Window::new(egui::WidgetText::from("Confirm Discard"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(window_frame)
                .show(ctx, |ui| {
                    confirm_icon(ui, tokens::DANGER_TEXT);
                    let msg = if is_untracked {
                        format!("Permanently delete untracked file '{}'?", path)
                    } else {
                        format!("Discard all unstaged changes in '{}'?", path)
                    };
                    ui.label(
                        RichText::new(msg)
                            .size(metrics::TEXT_BODY)
                            .color(tokens::TEXT),
                    );
                    ui.add_space(metrics::SP_SM);
                    ui.horizontal(|ui| {
                        if widgets::text_button(
                            ui,
                            None,
                            "Yes, Discard",
                            Btn::Danger,
                            true,
                        )
                        .clicked()
                        {
                            self.show_discard_dialog = None;
                            let p = path.clone();
                            self.run_async_op("Discard Changes", "Discarding changes...", move |dir| {
                                git::discard_file_changes(dir, &p, is_untracked)?;
                                Ok(format!("Discarded changes in '{}'", p))
                            });
                        }
                        if widgets::text_button(ui, None, "Cancel", Btn::Soft, true).clicked() {
                            self.show_discard_dialog = None;
                        }
                    });
                });
        }

        // Modal: Confirm Delete Branch
        if let Some(branch_name) = self.show_delete_branch_dialog.clone() {
            egui::Window::new(egui::WidgetText::from("Confirm Delete Branch"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(window_frame)
                .show(ctx, |ui| {
                    confirm_icon(ui, tokens::DANGER_TEXT);
                    ui.label(
                        RichText::new(format!(
                            "Are you sure you want to delete branch '{}'?",
                            branch_name
                        ))
                        .size(metrics::TEXT_BODY)
                        .color(tokens::TEXT),
                    );
                    ui.add_space(metrics::SP_SM);
                    ui.horizontal(|ui| {
                        if widgets::text_button(ui, None, "Yes, Delete", Btn::Danger, true)
                            .clicked()
                        {
                            self.show_delete_branch_dialog = None;
                            let b = branch_name.clone();
                            self.run_async_op("Delete Branch", "Deleting branch...", move |dir| {
                                git::delete_branch(dir, &b, true)?;
                                Ok(format!("Deleted branch '{}'", b))
                            });
                        }
                        if widgets::text_button(ui, None, "Cancel", Btn::Soft, true).clicked() {
                            self.show_delete_branch_dialog = None;
                        }
                    });
                });
        }

        // Modal: Connect Remote
        if self.show_connect_remote_modal {
            let mut close_modal = false;
            let mut do_connect = false;
            let mut remote_to_delete = None;

            egui::Window::new(egui::WidgetText::from("Connect Remote Repository"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(window_frame)
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.label(
                        RichText::new("Link this local repository with a remote (GitHub, GitLab, etc.):")
                            .size(metrics::TEXT_BODY)
                            .color(tokens::TEXT),
                    );
                    ui.add_space(metrics::SP_SM);

                    if !self.remotes.is_empty() {
                        ui.label(
                            RichText::new("Existing Remotes:")
                                .strong()
                                .size(metrics::TEXT_SMALL)
                                .color(tokens::TEXT_MUTED),
                        );
                        for r in &self.remotes {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&r.name).strong().color(tokens::ACCENT));
                                ui.label(RichText::new(&r.url).size(metrics::TEXT_SMALL).color(tokens::TEXT_MUTED).monospace());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if widgets::text_button(ui, None, "Delete", Btn::Quiet, true).clicked() {
                                        remote_to_delete = Some(r.name.clone());
                                    }
                                });
                            });
                        }
                        ui.add_space(metrics::SP_SM);
                        ui.separator();
                        ui.add_space(metrics::SP_SM);
                    }

                    ui.label(
                        RichText::new("Remote Name:")
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_name_input)
                            .desired_width(400.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("origin"),
                    );
                    ui.add_space(metrics::SP_XS);

                    ui.label(
                        RichText::new("Remote URL (HTTPS or SSH):")
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_url_input)
                            .desired_width(400.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("https://github.com/user/repository.git"),
                    );
                    ui.add_space(metrics::SP_SM);

                    ui.checkbox(&mut self.remote_fetch_after, "Fetch remote branches immediately");
                    ui.checkbox(&mut self.remote_push_upstream, "Push and set current branch as upstream tracking");

                    ui.add_space(metrics::SP_MD);
                    ui.horizontal(|ui| {
                        let can_connect = !self.remote_name_input.trim().is_empty()
                            && !self.remote_url_input.trim().is_empty();

                        if widgets::text_button(
                            ui,
                            Some(icons::Icon::Link),
                            "Connect & Link",
                            Btn::Primary,
                            can_connect,
                        )
                        .clicked()
                        {
                            do_connect = true;
                        }
                        if widgets::text_button(ui, None, "Cancel", Btn::Soft, true).clicked() {
                            close_modal = true;
                        }
                    });
                });

            if let Some(r_name) = remote_to_delete {
                self.run_async_op("Remove Remote", "Removing remote...", move |dir| {
                    git::remove_remote(dir, &r_name)?;
                    Ok(format!("Removed remote '{}'", r_name))
                });
            }

            if close_modal {
                self.show_connect_remote_modal = false;
            }

            if do_connect {
                self.show_connect_remote_modal = false;
                let name = self.remote_name_input.trim().to_string();
                let url = self.remote_url_input.trim().to_string();
                let fetch = self.remote_fetch_after;
                let push_up = self.remote_push_upstream;
                let cur_branch = self.current_branch.clone();

                self.run_async_op("Connect Remote", "Connecting remote repository...", move |dir| {
                    git::add_or_set_remote(dir, &name, &url)?;
                    let mut msgs = vec![format!("Connected remote '{}' ({})", name, url)];
                    if fetch {
                        match git::fetch(dir) {
                            Ok(_) => msgs.push("Fetched remote branches".into()),
                            Err(e) => msgs.push(format!("Fetch note: {}", e)),
                        }
                    }
                    if push_up && !cur_branch.is_empty() && !cur_branch.contains("(unborn)") {
                        match git::push_set_upstream(dir, &name, &cur_branch) {
                            Ok(_) => msgs.push(format!("Pushed & tracking '{}/{}'", name, cur_branch)),
                            Err(e) => msgs.push(format!("Upstream push note: {}", e)),
                        }
                    }
                    Ok(msgs.join(". "))
                });
            }
        }

        // Modal: .gitignore Configuration
        if self.show_gitignore_modal {
            let mut close_modal = false;
            let mut do_save = false;

            egui::Window::new(egui::WidgetText::from(".gitignore Configuration"))
                .collapsible(false)
                .resizable(true)
                .default_size(Vec2::new(540.0, 480.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(window_frame)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("Configure patterns and files to ignore in Git:")
                            .size(metrics::TEXT_BODY)
                            .color(tokens::TEXT),
                    );
                    ui.add_space(metrics::SP_XS);

                    if let Some(r) = &self.repo_path {
                        ui.label(
                            RichText::new(format!("Location: {}/.gitignore", r.display()))
                                .size(metrics::TEXT_SMALL)
                                .color(tokens::TEXT_MUTED)
                                .monospace(),
                        );
                    }
                    ui.add_space(metrics::SP_SM);

                    // Quick Add Pattern
                    ui.label(
                        RichText::new("Quick Add Pattern:")
                            .strong()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );
                    ui.horizontal(|ui| {
                        let text_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.gitignore_new_pattern)
                                .desired_width(340.0)
                                .font(egui::TextStyle::Monospace)
                                .hint_text("e.g. *.log, /target, .env, *.tmp"),
                        );
                        let can_add = !self.gitignore_new_pattern.trim().is_empty();
                        let enter_pressed = text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if widgets::text_button(ui, Some(icons::Icon::Plus), "Add Pattern", Btn::Soft, can_add).clicked() || (enter_pressed && can_add) {
                            let pat = self.gitignore_new_pattern.trim().to_string();
                            if !self.gitignore_content.ends_with('\n') && !self.gitignore_content.is_empty() {
                                self.gitignore_content.push('\n');
                            }
                            self.gitignore_content.push_str(&pat);
                            self.gitignore_content.push('\n');
                            self.gitignore_new_pattern.clear();
                        }
                    });
                    ui.add_space(metrics::SP_SM);

                    // Presets
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Quick Presets:").size(metrics::TEXT_SMALL).color(tokens::TEXT_MUTED));
                        if widgets::text_button(ui, None, "+ Rust", Btn::Quiet, true).clicked() {
                            if !self.gitignore_content.ends_with('\n') && !self.gitignore_content.is_empty() {
                                self.gitignore_content.push('\n');
                            }
                            self.gitignore_content.push_str("# Rust\n/target/\n**/*.rs.bk\nCargo.lock\n");
                        }
                        if widgets::text_button(ui, None, "+ Node.js", Btn::Quiet, true).clicked() {
                            if !self.gitignore_content.ends_with('\n') && !self.gitignore_content.is_empty() {
                                self.gitignore_content.push('\n');
                            }
                            self.gitignore_content.push_str("# Node\nnode_modules/\nnpm-debug.log*\nyarn-debug.log*\nyarn-error.log*\n.env\n/dist/\n");
                        }
                        if widgets::text_button(ui, None, "+ Python", Btn::Quiet, true).clicked() {
                            if !self.gitignore_content.ends_with('\n') && !self.gitignore_content.is_empty() {
                                self.gitignore_content.push('\n');
                            }
                            self.gitignore_content.push_str("# Python\n__pycache__/\n*.py[cod]\n*$py.class\nvenv/\n.env\n");
                        }
                        if widgets::text_button(ui, None, "+ OS & IDEs", Btn::Quiet, true).clicked() {
                            if !self.gitignore_content.ends_with('\n') && !self.gitignore_content.is_empty() {
                                self.gitignore_content.push('\n');
                            }
                            self.gitignore_content.push_str("# OS & IDEs\n.DS_Store\nThumbs.db\n.vscode/\n.idea/\n*.swp\n");
                        }
                    });
                    ui.add_space(metrics::SP_SM);

                    ui.label(
                        RichText::new(".gitignore Rules (editable):")
                            .strong()
                            .size(metrics::TEXT_SMALL)
                            .color(tokens::TEXT_MUTED),
                    );

                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.gitignore_content)
                                    .desired_width(ui.available_width())
                                    .desired_rows(12)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor(),
                            );
                        });

                    ui.add_space(metrics::SP_MD);
                    ui.horizontal(|ui| {
                        if widgets::text_button(
                            ui,
                            Some(icons::Icon::Check),
                            "Save .gitignore",
                            Btn::Primary,
                            true,
                        )
                        .clicked()
                        {
                            do_save = true;
                        }
                        if widgets::text_button(ui, None, "Cancel", Btn::Soft, true).clicked() {
                            close_modal = true;
                        }
                    });
                });

            if close_modal {
                self.show_gitignore_modal = false;
            }

            if do_save {
                self.show_gitignore_modal = false;
                if let Some(repo) = self.repo_path.clone() {
                    match git::save_gitignore(&repo, &self.gitignore_content) {
                        Ok(_) => {
                            self.notify_success("Gitignore", "Saved .gitignore successfully");
                            self.trigger_refresh();
                        }
                        Err(e) => {
                            self.notify_error("Gitignore", format!("Failed to save .gitignore: {}", e));
                        }
                    }
                }
            }
        }
    }

    fn render_toast_alert(&mut self, ctx: &egui::Context) {
        if let Some(toast) = &self.toast {
            if toast.created_at.elapsed().as_secs_f32() > 4.5 {
                self.toast = None;
                return;
            }
            // Request repaint so it will dismiss after 4.5s even if idle
            ctx.request_repaint();

            let is_error = toast.is_error;
            let (border_color, icon, icon_bg) = if is_error {
                (tokens::DANGER, icons::Icon::Warn, tokens::DEL_BG)
            } else {
                (tokens::SUCCESS, icons::Icon::Check, tokens::ADD_BG)
            };

            let mut dismiss = false;
            egui::Area::new(egui::Id::NULL.with("toast_alert_floating_area"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-20.0, 56.0))
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(tokens::SURFACE_HI)
                        .stroke(egui::Stroke::new(1.5, border_color))
                        .corner_radius(tokens::RADIUS_WINDOW)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
                                ui.painter().rect_filled(icon_rect, 12.0, icon_bg);
                                icons::paint(
                                    ui.painter(),
                                    icons::glyph_in(icon_rect, 14.0),
                                    icon,
                                    if is_error { tokens::DANGER_TEXT } else { tokens::SUCCESS },
                                );

                                ui.add_space(metrics::SP_2XS);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&toast.title)
                                                .strong()
                                                .size(metrics::TEXT_BODY)
                                                .color(tokens::TEXT),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let close_resp = ui.add(
                                                egui::Button::new(
                                                    RichText::new("x")
                                                        .size(metrics::TEXT_SMALL)
                                                        .color(tokens::TEXT_MUTED),
                                                )
                                                .frame(false),
                                            );
                                            if close_resp.clicked() {
                                                dismiss = true;
                                            }
                                        });
                                    });
                                    if !toast.message.is_empty() {
                                        ui.label(
                                            RichText::new(&toast.message)
                                                .size(metrics::TEXT_SMALL)
                                                .color(tokens::TEXT_MUTED),
                                        );
                                    }
                                });
                            });
                        });
                });

            if dismiss {
                self.toast = None;
            }
        }
    }
}

/// Warn icon of confirmation dialogs (no emoji, goal 30a).
fn confirm_icon(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::splat(metrics::ICON_LOGO * 1.5),
        egui::Sense::hover(),
    );
    icons::paint(
        ui.painter(),
        icons::glyph_in(rect, metrics::ICON_LOGO * 1.2),
        icons::Icon::Warn,
        color,
    );
}

/// Empty state: what belongs here, the one action that fills it (app scope).
fn empty_state(ui: &mut egui::Ui, icon: icons::Icon, caption: &str) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(metrics::SP_XL);
            let (icon_rect, _) = ui.allocate_exact_size(
                Vec2::splat(metrics::ICON_LOGO * 2.5),
                egui::Sense::hover(),
            );
            icons::paint(
                ui.painter(),
                icons::glyph_in(icon_rect, metrics::ICON_LOGO * 2.0),
                icon,
                tokens::TEXT_FAINT,
            );
            ui.add_space(metrics::SP_SM);
            ui.label(
                RichText::new(caption)
                    .weak()
                    .size(metrics::TEXT_BODY)
                    .color(tokens::TEXT_MUTED),
            );
        });
    });
}

// ---------------------------------------------------------------------------
// Row widgets — the list language: chip, name, right-side vector actions.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Stage,
    Unstage,
    Discard,
    Ignore,
}

struct RowBtn {
    icon: icons::Icon,
    action: RowAction,
    hover: &'static str,
}

/// One file row: status chip, path, right-aligned icon actions in fixed cells.
/// Returns `(row_clicked, action)`; an action wins over a row click.
fn file_row(
    ui: &mut egui::Ui,
    row_id: egui::Id,
    path: &str,
    status_char: char,
    selected: bool,
    btns: &[RowBtn],
) -> (bool, Option<RowAction>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(w, metrics::ROW_H),
        egui::Sense::hover(),
    );

    let btn_size = 22.0;
    let right_pad = 12.0; // Margin to avoid scrollbar overlap
    let total_btn_w = btns.len() as f32 * (btn_size + 4.0) + right_pad;
    let click_w = (rect.width() - total_btn_w).max(0.0);
    let row_click_rect = egui::Rect::from_min_size(
        rect.min,
        Vec2::new(click_w, rect.height()),
    );
    let row_resp = ui.interact(row_click_rect, row_id.with("row_click"), egui::Sense::click());

    let row_hover = row_resp.hovered();
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::ACCENT_BG);
    } else if row_hover {
        p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::SURFACE);
    }
    if row_resp.has_focus() {
        p.rect_stroke(
            rect.shrink(0.5),
            tokens::RADIUS_CHIP,
            egui::Stroke::new(1.0, tokens::ACCENT),
            egui::StrokeKind::Inside,
        );
    }

    // Status chip: clean 20x18 badge with centered letter and subtle border
    let st = crate::theme::status_style(status_char);
    let chip_h = 18.0;
    let chip_w = 20.0;
    let chip_r = egui::Rect::from_min_size(
        egui::Pos2::new(rect.left() + metrics::SP_2XS, rect.center().y - chip_h * 0.5),
        Vec2::new(chip_w, chip_h),
    );
    p.rect_filled(chip_r, tokens::RADIUS_CHIP, st.bg);
    p.rect_stroke(
        chip_r.shrink(0.5),
        tokens::RADIUS_CHIP,
        egui::Stroke::new(1.0, st.bar),
        egui::StrokeKind::Inside,
    );
    p.text(
        chip_r.center(),
        egui::Align2::CENTER_CENTER,
        status_char.to_string(),
        egui::FontId::monospace(metrics::TEXT_SMALL),
        st.fg,
    );

    // Right-side icon actions in fixed cells with right padding to clear scrollbar
    let mut action = None;
    let mut x = rect.right() - right_pad - btn_size;
    for (i, btn) in btns.iter().enumerate().rev() {
        let brect = egui::Rect::from_min_size(
            egui::Pos2::new(x, rect.center().y - btn_size * 0.5),
            Vec2::splat(btn_size),
        );
        let brep = ui.interact(brect, row_id.with(("action_btn", i)), egui::Sense::click());
        if brep.clicked() {
            action = Some(btn.action);
        }
        let brep = brep.on_hover_text(btn.hover);
        paint_hit_cell(
            p,
            brect,
            brep.hovered(),
            brep.is_pointer_button_down_on(),
            brep.has_focus(),
            tokens::SURFACE_HI,
            if brep.hovered() { tokens::TEXT } else { tokens::TEXT_MUTED },
        );
        icons::paint(
            p,
            icons::glyph_in(brect, metrics::ICON_SIZE - 2.0),
            btn.icon,
            if brep.hovered() { tokens::TEXT } else { tokens::TEXT_MUTED },
        );
        x -= btn_size + 4.0;
    }

    // Path truncated before buttons
    let text_x = chip_r.right() + metrics::SP_XS;
    let max_w = (click_w - (text_x - rect.left()) - metrics::SP_2XS).max(10.0);
    trunc_fit(
        p,
        egui::Pos2::new(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        path,
        metrics::TEXT_BODY,
        if selected { tokens::TEXT } else { tokens::TEXT },
        false,
        max_w,
    );

    let clicked = action.is_none() && row_resp.clicked();
    (clicked, action)
}

/// A branch row for the centre panel: glyph, name, upstream, actions.
/// Returns `(checkout_name, delete_name)`.
fn branch_row(
    ui: &mut egui::Ui,
    row_id: egui::Id,
    name: &str,
    upstream: Option<&str>,
    is_current: bool,
    is_remote: bool,
) -> (Option<String>, Option<String>) {
    let glyph = if is_remote {
        (icons::Icon::File, tokens::INFO)
    } else {
        (icons::Icon::Branch, if is_current { tokens::ACCENT } else { tokens::TEXT_FAINT })
    };
    let font = egui::FontId::proportional(metrics::TEXT_BODY);
    let row_w = ui.available_width();
    let row_h = metrics::ROW_H + 6.0;
    let (rect, row) = ui.allocate_exact_size(
        Vec2::new(row_w, row_h),
        egui::Sense::click(),
    );
    let p = &ui.painter_at(rect);

    if is_current {
        p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::ACCENT_BG);
    } else if row.hovered() {
        p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::SURFACE);
    }

    // Branch icon on left
    icons::paint(
        p,
        icons::glyph_in(
            egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + metrics::SP_SM, rect.center().y - metrics::ICON_SIZE * 0.5),
                Vec2::splat(metrics::ICON_SIZE),
            ),
            metrics::ICON_SIZE,
        ),
        glyph.0,
        glyph.1,
    );

    let mut checkout = None;
    let mut delete = None;

    // Right-aligned actions
    let mut cur_right = rect.right() - metrics::SP_SM;

    if !is_current {
        // Trash button (delete branch)
        let trash_w = metrics::CTRL_H_SM;
        let trect = egui::Rect::from_min_size(
            egui::Pos2::new(cur_right - trash_w, rect.center().y - metrics::ROW_H * 0.5),
            Vec2::new(trash_w, metrics::ROW_H),
        );
        let trep = ui.interact(trect, row_id.with(1), egui::Sense::click());
        paint_hit_cell(
            p,
            trect,
            trep.hovered(),
            trep.is_pointer_button_down_on(),
            trep.has_focus(),
            tokens::SURFACE_HI,
            if trep.hovered() { tokens::DANGER_TEXT } else { tokens::TEXT_MUTED },
        );
        icons::paint(
            p,
            icons::glyph_in(trect, metrics::ICON_SIZE - 2.0),
            icons::Icon::Trash,
            if trep.hovered() { tokens::DANGER_TEXT } else { tokens::TEXT_MUTED },
        );
        if trep.on_hover_text("Delete branch").clicked() {
            delete = Some(name.to_string());
        }
        cur_right -= trash_w + metrics::SP_XS;

        // Checkout button
        let label = if is_remote { "Track / Checkout" } else { "Checkout" };
        let btn_text_w = p.layout_no_wrap(label.to_owned(), font.clone(), tokens::TEXT).size().x;
        let btn_w = btn_text_w + 16.0;
        let btn_rect = egui::Rect::from_min_size(
            egui::Pos2::new(cur_right - btn_w, rect.center().y - metrics::ROW_H * 0.5),
            Vec2::new(btn_w, metrics::ROW_H),
        );
        let brep = ui.interact(btn_rect, row_id.with(0), egui::Sense::click());
        paint_hit_cell(
            p,
            btn_rect,
            brep.hovered(),
            brep.is_pointer_button_down_on(),
            brep.has_focus(),
            tokens::SURFACE_HI,
            if brep.hovered() { tokens::TEXT } else { tokens::TEXT_MUTED },
        );
        p.text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            if brep.hovered() { tokens::TEXT } else { tokens::TEXT_MUTED },
        );
        if brep.on_hover_text(label).clicked() {
            checkout = Some(name.to_string());
        }
        cur_right -= btn_w + metrics::SP_SM;
    }

    // Upstream tracking text
    if let Some(up) = upstream {
        let up_str = format!("-> {}", up);
        let up_w = p.layout_no_wrap(up_str.clone(), egui::FontId::proportional(metrics::TEXT_SMALL), tokens::TEXT_FAINT).size().x;
        widgets::paint_text(
            p,
            egui::Pos2::new(cur_right - up_w, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &up_str,
            metrics::TEXT_SMALL,
            tokens::TEXT_FAINT,
            false,
        );
        cur_right -= up_w + metrics::SP_MD;
    }

    // Branch name & "current" badge
    let text_x = rect.left() + metrics::SP_SM + metrics::ICON_SIZE + metrics::SP_SM;
    let name_font = egui::FontId::proportional(metrics::TEXT_BODY);
    let name_size = p.layout_no_wrap(name.to_owned(), name_font, tokens::TEXT).size().x;
    let badge_w = if is_current { 60.0 } else { 0.0 };
    let max_name_w = (cur_right - text_x - badge_w - metrics::SP_XS).max(20.0);

    let actual_name_w = name_size.min(max_name_w);
    trunc_fit(
        p,
        egui::Pos2::new(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        metrics::TEXT_BODY,
        if is_current { tokens::ACCENT } else { tokens::TEXT },
        false,
        max_name_w,
    );

    if is_current {
        let badge_x = text_x + actual_name_w + metrics::SP_SM;
        let badge_rect = egui::Rect::from_min_size(
            egui::Pos2::new(badge_x, rect.center().y - 9.0),
            Vec2::new(54.0, 18.0),
        );
        p.rect_filled(badge_rect, tokens::RADIUS_CHIP, tokens::ACCENT_BG);
        p.rect_stroke(
            badge_rect,
            tokens::RADIUS_CHIP,
            egui::Stroke::new(1.0, tokens::ACCENT),
            egui::StrokeKind::Middle,
        );
        p.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            "current",
            egui::FontId::proportional(metrics::TEXT_SMALL),
            tokens::ACCENT,
        );
    }

    (checkout, delete)
}

/// Paints a hit cell: hover well, focus ring.
fn paint_hit_cell(
    p: &egui::Painter,
    rect: egui::Rect,
    hovered: bool,
    pressed: bool,
    focus: bool,
    fill: egui::Color32,
    _text: egui::Color32,
) {
    if pressed {
        p.rect_filled(rect, tokens::RADIUS_CHIP, tokens::SURFACE_ACTIVE);
    } else if hovered {
        p.rect_filled(rect, tokens::RADIUS_CHIP, fill);
    }
    if focus {
        p.rect_stroke(
            rect,
            tokens::RADIUS_CHIP,
            egui::Stroke::new(1.5, tokens::ACCENT),
            egui::StrokeKind::Middle,
        );
    }
}

/// History row: summary on top, author left and date right on the baseline,
/// short hash right-aligned on the top line (no dotted meta joins, 63).
fn commit_row(ui: &mut egui::Ui, _row_id: egui::Id, c: &CommitInfo, selected: bool) -> bool {
    let w = ui.available_width();
    let row_h = 44.0;
    let (rect, row) = ui.allocate_exact_size(Vec2::new(w, row_h), egui::Sense::click());
    let p = &ui.painter_at(rect);

    if selected {
        p.rect_filled(rect, tokens::RADIUS_WIDGET, tokens::ACCENT_BG);
    } else if row.hovered() {
        p.rect_filled(rect, tokens::RADIUS_WIDGET, tokens::SURFACE);
    }
    if row.has_focus() {
        p.rect_stroke(
            rect,
            tokens::RADIUS_WIDGET,
            egui::Stroke::new(1.0, tokens::ACCENT),
            egui::StrokeKind::Middle,
        );
    }

    // Hash, right-aligned, top line.
    let hash_font = egui::FontId::monospace(metrics::TEXT_SMALL);
    let hash_w = p.layout_no_wrap(c.short_hash.clone(), hash_font, tokens::TEXT_FAINT).size().x;
    widgets::paint_text(
        p,
        egui::Pos2::new(rect.right() - metrics::SP_SM, rect.top() + 12.0),
        egui::Align2::RIGHT_CENTER,
        &c.short_hash,
        metrics::TEXT_SMALL,
        tokens::TEXT_FAINT,
        true,
    );

    let summary_max_w = (rect.width() - 2.0 * metrics::SP_SM - hash_w - metrics::SP_SM).max(10.0);
    trunc_fit(
        p,
        egui::Pos2::new(rect.left() + metrics::SP_SM, rect.top() + 12.0),
        egui::Align2::LEFT_CENTER,
        &c.summary,
        metrics::TEXT_BODY,
        if selected { tokens::TEXT } else { tokens::TEXT },
        false,
        summary_max_w,
    );

    // Bottom line: Date on right, Author on left.
    let date_font = egui::FontId::proportional(metrics::TEXT_SMALL);
    let date_w = p.layout_no_wrap(c.date.clone(), date_font, tokens::TEXT_FAINT).size().x;
    widgets::paint_text(
        p,
        egui::Pos2::new(rect.right() - metrics::SP_SM, rect.bottom() - 12.0),
        egui::Align2::RIGHT_CENTER,
        &c.date,
        metrics::TEXT_SMALL,
        tokens::TEXT_FAINT,
        false,
    );

    let author_max_w = (rect.width() - 2.0 * metrics::SP_SM - date_w - metrics::SP_SM).max(10.0);
    trunc_fit(
        p,
        egui::Pos2::new(rect.left() + metrics::SP_SM, rect.bottom() - 12.0),
        egui::Align2::LEFT_CENTER,
        &c.author_name,
        metrics::TEXT_SMALL,
        tokens::TEXT_MUTED,
        false,
        author_max_w,
    );

    row.clicked()
}

/// Paints `text` at `pos` truncated with an ellipsis so it never exceeds `max_w`.
fn trunc_fit(
    p: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    size: f32,
    color: egui::Color32,
    mono: bool,
    max_w: f32,
) -> egui::Rect {
    let font = if mono {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    };
    let mut s = text.to_string();
    let mut gal = p.layout_no_wrap(s.clone(), font.clone(), color);
    let count = text.chars().count();
    let mut take = count;
    while gal.size().x > max_w && take > 1 {
        take -= 1;
        let t: String = text.chars().take(take).collect();
        s = format!("{}\u{2026}", t);
        gal = p.layout_no_wrap(s.clone(), font.clone(), color);
    }
    // Anchor manually from the laid-out size.
    let rect = anchor.anchor_size(pos, gal.size());
    p.galley(rect.min, gal, color);
    rect
}

// ---------------------------------------------------------------------------
// Diff and code viewers — the signature. Contiguous painted rows over a well,
// two-column gutter, semantic kind colours, monospace material.
// ---------------------------------------------------------------------------

/// Monospace cell width (advance) of the current mono font.
fn mono_advance(ui: &egui::Ui) -> f32 {
    ui.painter()
        .layout_no_wrap("0".to_owned(), egui::FontId::monospace(metrics::TEXT_MONO), tokens::TEXT)
        .size()
        .x
}

fn paint_diff_row(
    p: &egui::Painter,
    x0: f32,
    y: f32,
    w: f32,
    line: &crate::models::DiffLine,
    bar_len: f32,
) {
    use crate::models::DiffLineKind;

    let (bg, bar, fg, sym, symbol_w) = match line.kind {
        DiffLineKind::Addition => (
            tokens::ADD_BG,
            Some(tokens::ADD_BAR),
            tokens::TEXT,
            ("+", tokens::SUCCESS),
            14.0,
        ),
        DiffLineKind::Deletion => (
            tokens::DEL_BG,
            Some(tokens::DEL_BAR),
            tokens::TEXT,
            ("-", tokens::DANGER_TEXT),
            14.0,
        ),
        DiffLineKind::HunkHeader => (
            tokens::HUNK_BG,
            None,
            tokens::TEXT_MUTED,
            ("@", tokens::ACCENT),
            14.0,
        ),
        DiffLineKind::Meta => (
            tokens::META_BG,
            None,
            tokens::TEXT_FAINT,
            (" ", tokens::TEXT_FAINT),
            14.0,
        ),
        DiffLineKind::Context => (
            egui::Color32::TRANSPARENT,
            None,
            tokens::TEXT,
            (" ", tokens::TEXT_FAINT),
            14.0,
        ),
    };

    // Row background.
    if bg != egui::Color32::TRANSPARENT {
        p.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::new(x0, y), Vec2::new(w, metrics::DIFF_ROW_H)),
            0.0,
            bg,
        );
    }
    // Kind bar on the left edge (green / red / none).
    if let Some(bar_color) = bar {
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::Pos2::new(x0, y),
                Vec2::new(bar_len, metrics::DIFF_ROW_H),
            ),
            0.0,
            bar_color,
        );
    }

    // Gutter is subtly darker than the colours above via the num paint only; a
    // light seam separates numbers from content.
    p.rect_filled(
        egui::Rect::from_min_size(
            egui::Pos2::new(x0 + bar_len, y),
            Vec2::new(92.0, metrics::DIFF_ROW_H),
        ),
        0.0,
        tokens::GUTTER_BG,
    );

    let cy = y + metrics::DIFF_ROW_H * 0.5;

    // Old / new line numbers, right-aligned columns.
    let old_str = line.old_line.map(|n| n.to_string()).unwrap_or_default();
    let new_str = line.new_line.map(|n| n.to_string()).unwrap_or_default();
    if !old_str.is_empty() {
        paint_num(p, x0 + bar_len + 4.0, x0 + bar_len + 42.0, cy, &old_str);
    }
    if !new_str.is_empty() {
        paint_num(p, x0 + bar_len + 50.0, x0 + bar_len + 88.0, cy, &new_str);
    }

    // Symbol column.
    p.text(
        egui::Pos2::new(x0 + bar_len + 96.0, cy),
        egui::Align2::LEFT_CENTER,
        sym.0,
        egui::FontId::monospace(metrics::TEXT_MONO),
        fg_symbol_color(line.kind),
    );

    // Line content, from a fixed offset so it aligns across rows.
    let content_x = x0 + bar_len + 96.0 + symbol_w + 4.0;
    if !line.content.is_empty() {
        let font = egui::FontId::monospace(metrics::TEXT_MONO);
        let gal = p.layout_no_wrap(line.content.clone(), font, fg);
        p.galley(egui::Pos2::new(content_x, cy - gal.size().y * 0.5), gal, fg);
    }
}

fn fg_symbol_color(kind: crate::models::DiffLineKind) -> egui::Color32 {
    use crate::models::DiffLineKind::*;
    match kind {
        Addition => tokens::SUCCESS,
        Deletion => tokens::DANGER_TEXT,
        HunkHeader => tokens::ACCENT,
        Meta | Context => tokens::TEXT_FAINT,
    }
}

fn paint_num(p: &egui::Painter, x_left: f32, x_right: f32, cy: f32, s: &str) {
    let font = egui::FontId::monospace(metrics::TEXT_SMALL);
    let gal = p.layout_no_wrap(s.to_owned(), font, tokens::GUTTER_NUM);
    p.galley(
        egui::Pos2::new(x_right - gal.size().x, cy - gal.size().y * 0.5),
        gal,
        tokens::GUTTER_NUM,
    );
    let _ = x_left;
}

fn render_diff_viewer(ui: &mut egui::Ui, diff: &ParsedDiff) {
    let bars = 2.0;
    let advance = mono_advance(ui);

    // Monospace width from precomputed max line length (O(1))
    let content_w: f32 = 96.0 + bars + 16.0 + (diff.max_line_len as f32 * advance);
    let row_h = metrics::DIFF_ROW_H;
    let total_h = diff.lines.len() as f32 * row_h;

    egui::ScrollArea::both()
        .id_salt("diff_content_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            let w = content_w.max(ui.available_width());
            let h = total_h.max(ui.available_height());
            let (area, _resp) = ui.allocate_exact_size(
                Vec2::new(w, h),
                egui::Sense::hover(),
            );
            let p = &ui.painter_at(area);
            p.rect_filled(area, 0.0, tokens::WELL);
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::Pos2::new(area.left() + bars, area.top()),
                    Vec2::new(92.0, h),
                ),
                0.0,
                tokens::GUTTER_BG,
            );

            // Viewport culling: only layout and render rows visible on screen (O(1) frame time)
            let clip = ui.clip_rect();
            let first_idx = if clip.top() > area.top() {
                ((clip.top() - area.top()) / row_h).floor().max(0.0) as usize
            } else {
                0
            };
            let last_idx = if clip.bottom() > area.top() {
                (((clip.bottom() - area.top()) / row_h).ceil() + 3.0) as usize
            } else {
                0
            };
            let last_idx = last_idx.min(diff.lines.len());

            for i in first_idx..last_idx {
                paint_diff_row(p, area.left(), area.top() + i as f32 * row_h, w, &diff.lines[i], bars);
            }
        });
}

fn render_code_viewer(ui: &mut egui::Ui, lines: &[String], max_len: usize) {
    let advance = mono_advance(ui);
    let content_w: f32 = 52.0 + 8.0 + (max_len as f32 * advance);
    let row_h = metrics::DIFF_ROW_H;
    let total_h = lines.len() as f32 * row_h;

    egui::ScrollArea::both()
        .id_salt("code_content_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            let w = content_w.max(ui.available_width());
            let h = total_h.max(ui.available_height());
            let (area, _resp) = ui.allocate_exact_size(
                Vec2::new(w, h),
                egui::Sense::hover(),
            );
            let p = &ui.painter_at(area);
            p.rect_filled(area, 0.0, tokens::WELL);
            p.rect_filled(
                egui::Rect::from_min_size(area.min, Vec2::new(52.0, h)),
                0.0,
                tokens::GUTTER_BG,
            );

            // Viewport culling: only layout and render rows visible on screen
            let clip = ui.clip_rect();
            let first_idx = if clip.top() > area.top() {
                ((clip.top() - area.top()) / row_h).floor().max(0.0) as usize
            } else {
                0
            };
            let last_idx = if clip.bottom() > area.top() {
                (((clip.bottom() - area.top()) / row_h).ceil() + 3.0) as usize
            } else {
                0
            };
            let last_idx = last_idx.min(lines.len());

            for idx in first_idx..last_idx {
                let line = &lines[idx];
                let y = area.top() + idx as f32 * row_h;
                let cy = y + row_h * 0.5;
                // Line number.
                let num = (idx + 1).to_string();
                let font = egui::FontId::monospace(metrics::TEXT_SMALL);
                let gal = p.layout_no_wrap(num, font, tokens::GUTTER_NUM);
                p.galley(
                    egui::Pos2::new(area.left() + 48.0 - gal.size().x, cy - gal.size().y * 0.5),
                    gal,
                    tokens::GUTTER_NUM,
                );
                // Content.
                if !line.is_empty() {
                    let font = egui::FontId::monospace(metrics::TEXT_MONO);
                    let gal = p.layout_no_wrap(line.clone(), font, tokens::TEXT);
                    p.galley(
                        egui::Pos2::new(
                            area.left() + 52.0 + 4.0,
                            cy - gal.size().y * 0.5,
                        ),
                        gal,
                        tokens::TEXT,
                    );
                }
            }
        });
}

// Recursive directory tree renderer with cache вЂ” folders carry the collapsing arrow,
// files carry the vector file glyph.
fn render_dir_tree(
    ui: &mut egui::Ui,
    root: &Path,
    current_dir: &Path,
    dir_cache: &mut std::collections::HashMap<PathBuf, (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>)>,
    file_to_read: &mut Option<String>,
) {
    let (dirs, files) = dir_cache.entry(current_dir.to_path_buf()).or_insert_with(|| {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }

                if path.is_dir() {
                    dirs.push((name, path));
                } else {
                    files.push((name, path));
                }
            }

            dirs.sort_by(|a, b| a.0.cmp(&b.0));
            files.sort_by(|a, b| a.0.cmp(&b.0));
        }

        (dirs, files)
    });

    let dirs = dirs.clone();
    let files = files.clone();

    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

    for (name, path) in dirs {
        ui.collapsing(RichText::new(name.clone()).color(tokens::TEXT_MUTED), |ui| {
            render_dir_tree(ui, root, &path, dir_cache, file_to_read);
        });
    }

    for (name, path) in files {
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy().to_string();
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(
                    Vec2::splat(metrics::ICON_SIZE - 2.0),
                    egui::Sense::hover(),
                );
                icons::paint(
                    ui.painter(),
                    icons::glyph_in(icon_rect, metrics::ICON_SIZE - 4.0),
                    icons::Icon::File,
                    tokens::TEXT_FAINT,
                );
                if ui.selectable_label(false, RichText::new(name).color(tokens::TEXT)).clicked() {
                    *file_to_read = Some(rel_str);
                }
            });
        }
    }
}