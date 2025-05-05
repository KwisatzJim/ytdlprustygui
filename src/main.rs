// main.rs
use eframe::{egui, NativeOptions};
use egui::{Button, CentralPanel, Color32, ComboBox, RichText, Vec2};
use rfd::FileDialog;
use std::{
    error::Error,
    path::PathBuf,
    process::Command,
    thread,
};
use std::sync::mpsc::{channel, Receiver};

struct YtDlpGUI {
    url: String,
    output_dir: String,
    selected_video_format: String,
    selected_audio_format: String,
    available_video_formats: Vec<Format>,
    available_audio_formats: Vec<Format>,
    status_message: String,
    status_color: Color32,
    is_processing: bool,
    format_receiver: Option<Receiver<Result<(Vec<Format>, Vec<Format>), String>>>,
    download_receiver: Option<Receiver<Result<(), String>>>,
    download_progress: f32,
    download_type: DownloadType,
}

#[derive(Debug, Clone, PartialEq)]
enum DownloadType {
    VideoAudio,  // Combined video+audio to MP4
    AudioOnly,   // Audio only as MP3
}

#[derive(Debug, Clone)]
struct Format {
    id: String,
    extension: String,
    resolution: String,
    description: String,
    is_video: bool,  // Flag to indicate if this is a video format
    is_audio: bool,  // Flag to indicate if this is an audio format
}

impl YtDlpGUI {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Default to user's home directory for downloads
        let output_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        Self {
            url: String::new(),
            output_dir,
            selected_video_format: String::new(),
            selected_audio_format: String::new(),
            available_video_formats: Vec::new(),
            available_audio_formats: Vec::new(),
            status_message: String::from("Ready"),
            status_color: Color32::GRAY,
            is_processing: false,
            format_receiver: None,
            download_receiver: None,
            download_progress: 0.0,
            download_type: DownloadType::VideoAudio,
        }
    }

    fn fetch_formats(&mut self) {
        if self.url.is_empty() {
            self.set_status("Please enter a URL first", Color32::RED);
            return;
        }

        self.set_status("Fetching available formats...", Color32::YELLOW);
        self.is_processing = true;
        
        // Clone values for the thread
        let url = self.url.clone();
        
        // Create a channel to receive results
        let (sender, receiver) = channel();
        self.format_receiver = Some(receiver);
        
        // Spawn a new thread to run yt-dlp
        thread::spawn(move || {
            // Run yt-dlp to get format information
            let output = Command::new("yt-dlp")
                .args(&["--list-formats", &url])
                .output();
                
            match output {
                Ok(output) => {
                    if !output.status.success() {
                        let error = String::from_utf8_lossy(&output.stderr).to_string();
                        sender.send(Err(format!("Failed to fetch formats: {}", error))).unwrap();
                        return;
                    }
                    
                    let output_str = String::from_utf8_lossy(&output.stdout).to_string();
                    let formats = parse_formats(&output_str);
                    
                    // Separate video and audio formats
                    let video_formats: Vec<Format> = formats.iter()
                        .filter(|f| f.is_video)
                        .cloned()
                        .collect();
                    
                    let audio_formats: Vec<Format> = formats.iter()
                        .filter(|f| f.is_audio)
                        .cloned()
                        .collect();
                    
                    sender.send(Ok((video_formats, audio_formats))).unwrap();
                },
                Err(e) => {
                    sender.send(Err(format!("Failed to execute yt-dlp: {}", e))).unwrap();
                }
            }
        });
    }
    
    fn download(&mut self) {
        if self.url.is_empty() {
            self.set_status("Please enter a URL first", Color32::RED);
            return;
        }
        
        if self.output_dir.is_empty() {
            self.set_status("Please select an output directory", Color32::RED);
            return;
        }
        
        // For video+audio mode, we need both formats selected
        if self.download_type == DownloadType::VideoAudio &&
           (self.selected_video_format.is_empty() || self.selected_audio_format.is_empty() ||
            self.available_video_formats.is_empty() || self.available_audio_formats.is_empty()) {
            self.set_status("Please fetch and select both video and audio formats", Color32::RED);
            return;
        }
        
        self.is_processing = true;
        self.download_progress = 0.0;
        
        // Prepare download command arguments
        let url = self.url.clone();
        let output_dir = self.output_dir.clone();
        let selected_video_format = self.selected_video_format.clone();
        let selected_audio_format = self.selected_audio_format.clone();
        let download_type = self.download_type.clone();
        
        self.set_status("Downloading...", Color32::YELLOW);
        
        // Create a channel for download results
        let (sender, receiver) = channel();
        self.download_receiver = Some(receiver);
        
        // Spawn a new thread for downloading
        thread::spawn(move || {
            let mut cmd = Command::new("yt-dlp");
            
            match download_type {
                DownloadType::VideoAudio => {
                    // For video+audio MP4 download with separate format selection
                    let format_spec = format!("{}+{}", selected_video_format, selected_audio_format);
                    cmd.args(&[
                        "-f", &format_spec,
                        "-o", &format!("{}/%(title)s.%(ext)s", output_dir),
                        "--merge-output-format", "mp4",
                        &url
                    ]);
                },
                DownloadType::AudioOnly => {
                    // For audio-only MP3 download
                    cmd.args(&[
                        "-x",
                        "--audio-format", "mp3",
                        "-o", &format!("{}/%(title)s.%(ext)s", output_dir),
                        &url
                    ]);
                }
            }
            
            match cmd.output() {
                Ok(output) => {
                    if output.status.success() {
                        sender.send(Ok(())).unwrap();
                    } else {
                        let error = String::from_utf8_lossy(&output.stderr).to_string();
                        sender.send(Err(format!("Download failed: {}", error))).unwrap();
                    }
                },
                Err(e) => {
                    sender.send(Err(format!("Failed to execute yt-dlp: {}", e))).unwrap();
                }
            }
        });
    }
    
    fn set_status(&mut self, message: &str, color: Color32) {
        self.status_message = message.to_string();
        self.status_color = color;
    }
    
    fn browse_output_dir(&mut self) {
        if let Some(folder) = FileDialog::new()
            .set_directory(&self.output_dir)
            .pick_folder() {
            self.output_dir = folder.to_string_lossy().to_string();
        }
    }
    
    fn handle_clipboard_paste(&mut self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                self.url = text;
            } else {
                self.set_status("Failed to paste from clipboard", Color32::RED);
            }
        } else {
            self.set_status("Clipboard access failed", Color32::RED);
        }
    }
    
    fn check_receivers(&mut self) {
        // Check format receiver
        if let Some(receiver) = &self.format_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok((video_formats, audio_formats)) => {
                        self.available_video_formats = video_formats;
                        self.available_audio_formats = audio_formats;
                        
                        // Set default selections if formats are available
                        if !self.available_video_formats.is_empty() {
                            self.selected_video_format = self.available_video_formats[0].id.clone();
                        }
                        
                        if !self.available_audio_formats.is_empty() {
                            self.selected_audio_format = self.available_audio_formats[0].id.clone();
                        }
                        
                        if !self.available_video_formats.is_empty() && !self.available_audio_formats.is_empty() {
                            self.set_status("Formats fetched successfully", Color32::GREEN);
                        } else {
                            self.set_status("No formats available or could not distinguish audio/video formats", Color32::RED);
                        }
                    },
                    Err(e) => {
                        self.set_status(&e, Color32::RED);
                    }
                }
                self.is_processing = false;
                self.format_receiver = None;
            }
        }
        
        // Check download receiver
        if let Some(receiver) = &self.download_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok(()) => {
                        self.set_status("Download completed successfully", Color32::GREEN);
                        self.download_progress = 1.0;
                    },
                    Err(e) => {
                        self.set_status(&e, Color32::RED);
                    }
                }
                self.is_processing = false;
                self.download_receiver = None;
            }
        }
    }
}

impl eframe::App for YtDlpGUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for updates from background threads
        self.check_receivers();
        
        CentralPanel::default().show(ctx, |ui| {
            ui.heading("YT-DLP Rusty GUI");
            
            // URL input with paste button
            ui.horizontal(|ui| {
                ui.label("URL:");
                ui.text_edit_singleline(&mut self.url).lost_focus();
                
                if ui.button("Paste").clicked() {
                    self.handle_clipboard_paste();
                }
            });
            
            // Output directory selection
            ui.horizontal(|ui| {
                ui.label("Output Directory:");
                ui.text_edit_singleline(&mut self.output_dir);
                
                if ui.button("Browse").clicked() {
                    self.browse_output_dir();
                }
            });
            
            // Download type selection
            ui.horizontal(|ui| {
                ui.label("Download Type:");
                
                ui.radio_value(&mut self.download_type, DownloadType::VideoAudio, "Video+Audio (MP4)");
                ui.radio_value(&mut self.download_type, DownloadType::AudioOnly, "Audio Only (MP3)");
            });
            
            // Format fetching and selection (only shown for video+audio mode)
            if self.download_type == DownloadType::VideoAudio {
                ui.horizontal(|ui| {
                    let fetch_button = ui.add_enabled(
                        !self.is_processing && !self.url.is_empty(),
                        Button::new("Fetch Formats")
                    );
                    
                    if fetch_button.clicked() {
                        self.fetch_formats();
                    }
                });
                
                // Only show format selection when formats are available
                if !self.available_video_formats.is_empty() && !self.available_audio_formats.is_empty() {
                    // Video format selection
                    ui.horizontal(|ui| {
                        ui.label("Video Format:");
                        
                        ComboBox::new("video_format_combo", "")
                            .selected_text(format!("{} ({})", 
                                self.selected_video_format,
                                self.available_video_formats.iter()
                                    .find(|f| f.id == self.selected_video_format)
                                    .map(|f| f.description.as_str())
                                    .unwrap_or("")
                            ))
                            .show_ui(ui, |ui| {
                                for format in &self.available_video_formats {
                                    ui.selectable_value(
                                        &mut self.selected_video_format, 
                                        format.id.clone(),
                                        format!("{} - {} ({})", format.id, format.resolution, format.extension)
                                    );
                                }
                            });
                    });
                    
                    // Audio format selection
                    ui.horizontal(|ui| {
                        ui.label("Audio Format:");
                        
                        ComboBox::new("audio_format_combo", "")
                            .selected_text(format!("{} ({})", 
                                self.selected_audio_format,
                                self.available_audio_formats.iter()
                                    .find(|f| f.id == self.selected_audio_format)
                                    .map(|f| f.description.as_str())
                                    .unwrap_or("")
                            ))
                            .show_ui(ui, |ui| {
                                for format in &self.available_audio_formats {
                                    ui.selectable_value(
                                        &mut self.selected_audio_format, 
                                        format.id.clone(),
                                        format!("{} - {}", format.id, format.description)
                                    );
                                }
                            });
                    });
                }
            }
            
            // Download button
            if ui.add_enabled(!self.is_processing, Button::new("Download")).clicked() {
                self.download();
            }
            
            // Progress indicator (simple for now)
            if self.is_processing {
                ui.spinner();
            }
            
            // Status message
            ui.horizontal(|ui| {
                ui.label("Status: ");
                ui.label(RichText::new(&self.status_message).color(self.status_color));
            });
            
            // Format list display
            if (!self.available_video_formats.is_empty() || !self.available_audio_formats.is_empty()) 
               && self.download_type == DownloadType::VideoAudio {
                // Video formats
                if !self.available_video_formats.is_empty() {
                    ui.collapsing("Available Video Formats", |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("ID").strong());
                            ui.add_space(50.0);
                            ui.label(RichText::new("Extension").strong());
                            ui.add_space(30.0);
                            ui.label(RichText::new("Resolution").strong());
                            ui.add_space(30.0);
                            ui.label(RichText::new("Description").strong());
                        });
                        
                        ui.separator();
                        
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for format in &self.available_video_formats {
                                ui.horizontal(|ui| {
                                    ui.label(&format.id);
                                    ui.add_space(50.0);
                                    ui.label(&format.extension);
                                    ui.add_space(30.0);
                                    ui.label(&format.resolution);
                                    ui.add_space(30.0);
                                    ui.label(&format.description);
                                });
                            }
                        });
                    });
                }
                
                // Audio formats
                if !self.available_audio_formats.is_empty() {
                    ui.collapsing("Available Audio Formats", |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("ID").strong());
                            ui.add_space(50.0);
                            ui.label(RichText::new("Extension").strong());
                            ui.add_space(30.0);
                            ui.label(RichText::new("Description").strong());
                        });
                        
                        ui.separator();
                        
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for format in &self.available_audio_formats {
                                ui.horizontal(|ui| {
                                    ui.label(&format.id);
                                    ui.add_space(50.0);
                                    ui.label(&format.extension);
                                    ui.add_space(30.0);
                                    ui.label(&format.description);
                                });
                            }
                        });
                    });
                }
            }
        });
        
        // Request repaint if we're processing to keep checking receivers
        if self.is_processing {
            ctx.request_repaint();
        }
    }
}

fn parse_formats(output: &str) -> Vec<Format> {
    let mut formats = Vec::new();
    
    // Flag to indicate we've reached the format table section
    let mut in_format_table = false;
    
    for line in output.lines() {
        // Skip lines until we find the format table header
        if line.contains("ID") && line.contains("EXT") && line.contains("RESOLUTION") {
            in_format_table = true;
            continue;
        }
        
        if !in_format_table {
            continue;
        }
        
        // Skip empty lines or lines without format information
        if line.trim().is_empty() || !line.contains(" ") {
            continue;
        }
        
        // Parse format line
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            // The format ID is always the first part
            let id = parts[0].to_string();
            
            // Extract extension (usually the second part)
            let extension = if parts.len() > 1 {
                parts[1].to_string()
            } else {
                "unknown".to_string()
            };
            
            // Extract resolution if available
            let resolution = if parts.len() > 2 && parts[2].contains("x") {
                parts[2].to_string()
            } else {
                "audio only".to_string()
            };
            
            // Join remaining parts as description
            let description = if parts.len() > 3 {
                parts[3..].join(" ")
            } else {
                String::new()
            };
            
            // Detect if this is a video or audio format
            let is_video = !resolution.contains("audio only") || 
                           line.to_lowercase().contains("video only") ||
                           (line.contains("mp4") && !line.to_lowercase().contains("audio only"));
                           
            let is_audio = resolution.contains("audio only") || 
                          line.to_lowercase().contains("audio only") ||
                          extension == "m4a" || extension == "mp3" || extension == "ogg" || extension == "opus";
            
            // Only add if it's a real format (not a header or separator)
            if !id.contains("-") && !id.contains("=") {
                formats.push(Format {
                    id,
                    extension,
                    resolution,
                    description,
                    is_video,
                    is_audio,
                });
            }
        }
    }
    
    formats
}
    


fn main() -> Result<(), Box<dyn Error>> {
    // Check if yt-dlp is installed
    match Command::new("yt-dlp").arg("--version").output() {
        Ok(_) => (),
        Err(_) => {
            eprintln!("Error: yt-dlp not found. Please install yt-dlp first.");
            return Err("yt-dlp not found".into());
        }
    }

    let mut options = NativeOptions::default();
    options.viewport.inner_size = Some(Vec2::new(800.0, 600.0));
    
    eframe::run_native(
        "YT-DLP Rusty GUI",
        options,
        Box::new(|cc| Ok(Box::new(YtDlpGUI::new(cc)))),
    )
    .map_err(|e| e.into())
}