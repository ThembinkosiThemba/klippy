/// Klippy
///
/// A lightweight clipboard manager built with Rust and egui.
/// This application allows users to store and manage multiple clipboard entries.
use chrono::{DateTime, Local};
use clipboard::{ClipboardContext, ClipboardProvider};
use directories::ProjectDirs;
use eframe::{egui, App, CreationContext, Frame};
use egui::{Color32, Context, RichText, Sense, Stroke, Vec2, ViewportBuilder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Represents a single clipboard entry with content and metadata
#[derive(Clone, Serialize, Deserialize)]
struct ClipboardEntry {
    /// The actual text content
    content: String,
    /// When the entry was created
    timestamp: DateTime<Local>,
    /// Whether this entry is pinned (won't be removed automatically)
    pinned: bool,
}

impl ClipboardEntry {
    /// Create a new clipboard entry with the current timestamp
    fn new(content: String) -> Self {
        Self {
            content,
            timestamp: Local::now(),
            pinned: false,
        }
    }

    /// Returns a formatted timestamp string
    fn formatted_time(&self) -> String {
        self.timestamp.format("%H:%M:%S").to_string()
    }

    /// Returns a preview of the content (truncated if too long)
    fn preview(&self) -> String {
        if self.content.len() > 50 {
            format!("{}...", &self.content[..47])
        } else {
            self.content.clone()
        }
    }
}

/// Represents the main application state
#[derive(Serialize, Deserialize)]
struct ClipboardManager {
    /// List of clipboard entries
    entries: Vec<ClipboardEntry>,
    /// Maximum number of entries to keep
    max_entries: usize,
    /// Path to save application data
    #[serde(skip)]
    save_path: Option<PathBuf>,
    /// Clipboard context for interaction with system clipboard
    #[serde(skip)]
    clipboard_ctx: Option<ClipboardContext>,
    /// Current clipboard content for change detection
    #[serde(skip)]
    current_clipboard: String,
    /// Search term for filtering entries
    #[serde(skip)]
    search_term: String,
    /// Status message to display
    #[serde(skip)]
    status_message: Option<(String, f32)>, // (message, timer)
    #[serde(skip)]
    show_settings_window: bool,
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 50,
            save_path: None,
            clipboard_ctx: ClipboardProvider::new().ok(),
            current_clipboard: String::new(),
            search_term: String::new(),
            status_message: None,
            show_settings_window: false,
        }
    }
}

impl ClipboardManager {
    /// Initialize the application with saved data if available
    fn new() -> Self {
        let mut app = Self::default();

        // Set up save path
        if let Some(proj_dirs) = ProjectDirs::from("com", "klippy", "klippy") {
            let config_dir = proj_dirs.config_dir();
            if !config_dir.exists() {
                let _ = fs::create_dir_all(config_dir);
            }

            app.save_path = Some(config_dir.join("data.json"));

            // Load saved data
            if let Some(path) = &app.save_path {
                if path.exists() {
                    if let Ok(data) = fs::read_to_string(path) {
                        if let Ok(loaded) = serde_json::from_str::<ClipboardManager>(&data) {
                            app.entries = loaded.entries;
                            app.max_entries = loaded.max_entries;
                        }
                    }
                }
            }
        }

        app
    }

    /// Save application data to disk
    fn save_data(&self) {
        if let Some(path) = &self.save_path {
            if let Ok(json) = serde_json::to_string(self) {
                let _ = fs::write(path, json);
            }
        }
    }

    /// Add a new entry to the clipboard history
    fn add_entry(&mut self, content: String) {
        // Don't add empty content or duplicates
        if content.trim().is_empty() || self.entries.iter().any(|e| e.content == content) {
            return;
        }

        self.entries.insert(0, ClipboardEntry::new(content));

        // Remove oldest entries if we exceed max_entries (unless pinned)
        while self.entries.len() > self.max_entries {
            // Find the oldest non-pinned entry
            if let Some(idx) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.pinned)
                .map(|(i, _)| i)
                .last()
            {
                self.entries.remove(idx);
            } else {
                // All entries are pinned, can't remove any
                break;
            }
        }

        // Save data after changes
        self.save_data();
    }

    /// Copy entry content to clipboard
    fn copy_to_clipboard(&mut self, content: &str) -> bool {
        if let Some(ctx) = &mut self.clipboard_ctx {
            if ctx.set_contents(content.to_owned()).is_ok() {
                self.current_clipboard = content.to_owned();
                self.set_status("Copied to clipboard", 2.0);
                return true;
            }
        }
        self.set_status("Failed to copy to clipboard", 2.0);
        false
    }

    /// Check for new clipboard content
    fn check_clipboard(&mut self) {
        if let Some(ctx) = &mut self.clipboard_ctx {
            if let Ok(content) = ctx.get_contents() {
                if !content.is_empty() && content != self.current_clipboard {
                    self.current_clipboard = content.clone();
                    self.add_entry(content);
                }
            }
        }
    }

    /// Set a status message with a timer
    fn set_status(&mut self, message: &str, timer: f32) {
        self.status_message = Some((message.to_owned(), timer));
    }

    /// Update the status message timer
    fn update_status(&mut self, ctx: &egui::Context) {
        let current_time = ctx.input(|i| i.time); // Time since start in seconds
        static mut LAST_TIME: f64 = 0.0;
        let dt = unsafe {
            let delta = current_time - LAST_TIME;
            LAST_TIME = current_time;
            delta as f32
        };

        if let Some((_message, timer)) = &mut self.status_message {
            *timer -= dt;

            if *timer <= 0.0 {
                self.status_message = None;
            }

            // Request a repaint to update the status message
            ctx.request_repaint();
        }
    }

    /// Remove an entry at the specified index
    fn remove_entry(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.save_data();
            self.set_status("Entry removed", 2.0);
        }
    }

    /// Toggle pinned status of an entry
    fn toggle_pin(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries[index].pinned = !self.entries[index].pinned;
            self.save_data();

            let status = if self.entries[index].pinned {
                "Entry pinned"
            } else {
                "Entry unpinned"
            };
            self.set_status(status, 2.0);
        }
    }

    /// Get filtered entries based on search term
    fn filtered_entries(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if self.search_term.is_empty() {
                    true
                } else {
                    entry
                        .content
                        .to_lowercase()
                        .contains(&self.search_term.to_lowercase())
                }
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn open_clips(&mut self) {
        if let Some(path) = &self.save_path {
            if let Some(parent) = path.parent() {
                if let Err(e) = open::that(parent) {
                    self.set_status(&format!("Failed to open directory: {}", e), 3.0);
                } else {
                    self.set_status("Opened storage directory", 2.0);
                }
            }
        } else {
            self.set_status("Storage path not available", 2.0);
        }
    }
}

impl App for ClipboardManager {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Check for new clipboard content
        self.check_clipboard();

        // Update status message timer
        self.update_status(ctx); // Use delta_time instead of dt // Use delta time from ctx

        // Top panel with search and status
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🔍 Search").strong());
                ui.add_sized(
                    [200.0, 28.0],
                    egui::TextEdit::singleline(&mut self.search_term),
                );

                if ui
                    .button("❌ Clear")
                    .on_hover_text("Clear search")
                    .clicked()
                {
                    self.search_term.clear();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("📋 {} items", self.entries.len())).weak());
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });

        // Bottom panel with status messages
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let status = self
                    .status_message
                    .as_ref()
                    .map(|(msg, _)| msg.as_str())
                    .unwrap_or("Ready");
                ui.label(RichText::new(status).color(Color32::LIGHT_BLUE));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙ Settings").clicked() {
                        self.show_settings_window = true;
                    }

                    if ui.button("🧹 Clear Unpinned").clicked() {
                        self.entries.retain(|e| e.pinned);
                        self.save_data();
                        self.set_status("Cleared unpinned", 2.0);
                    }

                    if ui.button("📂 Open Location").clicked() {
                        self.open_clips();
                    }
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Store the filtered indices in a local variable to avoid keeping
                // the immutable borrow of self active
                let filtered = self.filtered_entries(); // Get Vec<usize> of indices

                if filtered.is_empty() {
                    ui.centered_and_justified(|ui| {
                        if self.search_term.is_empty() {
                            ui.label("No clipboard entries yet. Copy something to add it here.");
                        } else {
                            ui.label("No matching entries found.");
                        }
                    });
                } else {
                    // Clone any data needed in closures to avoid borrowing self
                    for &idx in &filtered {
                        ui.push_id(idx, |ui| {
                            if idx >= self.entries.len() {
                                return;
                            }

                            let preview = self.entries[idx].preview();
                            let formatted_time = self.entries[idx].formatted_time();
                            let is_pinned = self.entries[idx].pinned;
                            let content = self.entries[idx].content.clone(); // Clone if needed for clipboard

                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 40.0),
                                Sense::click(),
                            );

                            // Draw entry background
                            let bg_color = if is_pinned {
                                Color32::from_rgb(50, 50, 60)
                            } else {
                                Color32::from_rgb(30, 30, 35)
                            };
                            ui.painter().rect_filled(rect, 4.0, bg_color);

                            // Handle click to copy
                            if response.clicked() {
                                // Use a separate method or closure that takes the content directly
                                self.copy_to_clipboard(&content);
                            }

                            let mut content_layout = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(rect.shrink(8.0))
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );

                            content_layout.horizontal(|ui| {
                                // Time
                                ui.label(RichText::new(formatted_time).color(Color32::LIGHT_GRAY));
                                ui.add_space(8.0);

                                // Content preview
                                ui.label(preview);

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // copy button
                                        if ui.button("📋").clicked() {
                                            self.copy_to_clipboard(&content);
                                        }

                                        // Delete button with index capture
                                        if ui.button("🗑").clicked() {
                                            if idx < self.entries.len() {
                                                // Validate index is still valid
                                                self.remove_entry(idx);
                                            }
                                        }

                                        // Pin button with index capture
                                        let pin_text = if is_pinned { "📌" } else { "📍" };
                                        if ui.button(pin_text).clicked() {
                                            if idx < self.entries.len() {
                                                // Validate index is still valid
                                                self.toggle_pin(idx);
                                            }
                                        }
                                    },
                                );
                            });

                            // Draw separator
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                Stroke::new(1.0, Color32::from_rgb(50, 50, 55)),
                            );
                        });
                    }
                }
            });
        });

        if self.show_settings_window {
            let mut show = self.show_settings_window;
            let mut max_entries = self.max_entries;
            let mut save = false;

            egui::Window::new("⚙ Settings")
                .open(&mut show)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Set max number of entries to retain:");
                    ui.add(
                        egui::DragValue::new(&mut max_entries)
                            .range(10..=500)
                            .speed(1),
                    );

                    ui.add_space(10.0);
                    if ui.button("✅ Save").clicked() {
                        save = true;
                    }
                });

            self.show_settings_window = show;
            if save {
                self.max_entries = max_entries;
                self.save_data();
                self.set_status("Settings saved", 2.0);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_data();
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Klippy",
        options,
        Box::new(|cc: &CreationContext| {
            let app = Box::new(ClipboardManager::new());
            cc.egui_ctx.request_repaint_after(Duration::from_secs(1));
            Ok(app)
        }),
    )
}
