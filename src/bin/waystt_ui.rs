use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use waystt::ipc::{default_socket_path, OutputMode};
use waystt::transcript_events::TranscriptEvent;

const MAX_FINALS: usize = 200;
const SUBSCRIBE_REQUEST: &[u8] = b"{\"id\":\"waystt-ui\",\"cmd\":\"continuous_subscribe\"}\n";

#[derive(Debug, Clone)]
enum Connection {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Debug, Clone)]
struct FinalEntry {
    seq: u64,
    refined_text: String,
    output: OutputMode,
}

#[derive(Debug, Clone)]
struct UiState {
    connection: Connection,
    daemon_state: String,
    provider: String,
    model: String,
    partial: String,
    finals: Vec<FinalEntry>,
    last_error: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            connection: Connection::Connecting,
            daemon_state: "-".to_string(),
            provider: "-".to_string(),
            model: "-".to_string(),
            partial: String::new(),
            finals: Vec::new(),
            last_error: None,
        }
    }
}

struct WaysttUiApp {
    state: Arc<Mutex<UiState>>,
}

impl WaysttUiApp {
    fn new(state: Arc<Mutex<UiState>>, socket_path: PathBuf, ctx: egui::Context) -> Self {
        spawn_reader_thread(state.clone(), socket_path, ctx);
        Self { state }
    }
}

impl eframe::App for WaysttUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = self
            .state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| UiState {
                connection: Connection::Disconnected("UI state lock poisoned".to_string()),
                ..UiState::default()
            });

        egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (label, color) = match &state.connection {
                    Connection::Connecting => ("Connecting".to_string(), egui::Color32::YELLOW),
                    Connection::Connected => ("Connected".to_string(), egui::Color32::GREEN),
                    Connection::Disconnected(message) => {
                        (format!("Disconnected: {message}"), egui::Color32::RED)
                    }
                };
                ui.colored_label(color, label);
                ui.separator();
                ui.label(format!("state: {}", state.daemon_state));
                ui.separator();
                ui.label(format!("{}/{}", state.provider, state.model));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(error) = &state.last_error {
                egui::Frame::default()
                    .fill(ui.visuals().error_fg_color.linear_multiply(0.12))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    });
                ui.add_space(8.0);
            }

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Live").strong());
                    ui.add_space(4.0);
                    if state.partial.is_empty() {
                        ui.label(
                            egui::RichText::new("(listening…)")
                                .heading()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else {
                        ui.label(egui::RichText::new(&state.partial).heading());
                    }
                });

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Finalized").strong());
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &state.finals {
                        ui.group(|ui| {
                            ui.label(&entry.refined_text);
                            ui.small(
                                egui::RichText::new(format!(
                                    "#{} - {}",
                                    entry.seq,
                                    output_label(entry.output)
                                ))
                                .color(ui.visuals().weak_text_color()),
                            );
                        });
                        ui.add_space(6.0);
                    }
                });
        });
    }
}

fn spawn_reader_thread(state: Arc<Mutex<UiState>>, socket_path: PathBuf, ctx: egui::Context) {
    thread::spawn(move || loop {
        update_state(&state, &ctx, |state| {
            state.connection = Connection::Connecting;
        });

        match UnixStream::connect(&socket_path) {
            Ok(mut stream) => {
                if let Err(err) = stream
                    .write_all(SUBSCRIBE_REQUEST)
                    .and_then(|_| stream.flush())
                {
                    update_state(&state, &ctx, |state| {
                        state.connection = Connection::Disconnected(err.to_string());
                    });
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }

                update_state(&state, &ctx, |state| {
                    state.connection = Connection::Connected;
                });

                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if let Ok(event) = serde_json::from_str::<TranscriptEvent>(&line) {
                                update_state(&state, &ctx, |state| apply_event(state, event));
                            }
                        }
                        Err(err) => {
                            update_state(&state, &ctx, |state| {
                                state.connection = Connection::Disconnected(err.to_string());
                            });
                            break;
                        }
                    }
                }

                update_state(&state, &ctx, |state| {
                    state.connection = Connection::Disconnected("stream closed".to_string());
                });
            }
            Err(err) => {
                update_state(&state, &ctx, |state| {
                    state.connection = Connection::Disconnected(err.to_string());
                });
            }
        }

        thread::sleep(Duration::from_secs(1));
    });
}

fn apply_event(state: &mut UiState, event: TranscriptEvent) {
    match event {
        TranscriptEvent::Partial { text, .. } => {
            state.partial = text;
        }
        TranscriptEvent::Final {
            seq,
            refined_text,
            output,
            ..
        } => {
            state.partial.clear();
            state.finals.push(FinalEntry {
                seq,
                refined_text,
                output,
            });
            if state.finals.len() > MAX_FINALS {
                let excess = state.finals.len() - MAX_FINALS;
                state.finals.drain(0..excess);
            }
        }
        TranscriptEvent::Error { message } => {
            state.last_error = Some(message);
        }
        TranscriptEvent::State {
            state: daemon_state,
            provider,
            model,
        } => {
            state.daemon_state = daemon_state;
            state.provider = provider;
            state.model = model;
        }
    }
}

fn update_state(
    state: &Arc<Mutex<UiState>>,
    ctx: &egui::Context,
    update: impl FnOnce(&mut UiState),
) {
    if let Ok(mut state) = state.lock() {
        update(&mut state);
    }
    ctx.request_repaint();
}

fn output_label(output: OutputMode) -> &'static str {
    match output {
        OutputMode::Stdout => "stdout",
        OutputMode::Clipboard => "clipboard",
        OutputMode::Type => "type",
        OutputMode::Wtype => "wtype",
        OutputMode::Ydotool => "ydotool",
    }
}

fn parse_socket_arg() -> Result<PathBuf, String> {
    let mut socket = None;
    let mut args = std::env::args_os().skip(1);

    while let Some(arg) = args.next() {
        if arg == "--socket" {
            let path = args
                .next()
                .ok_or_else(|| "--socket requires a path".to_string())?;
            socket = Some(PathBuf::from(path));
        } else {
            return Err(format!("unknown argument: {}", display_os(arg)));
        }
    }

    Ok(socket.unwrap_or_else(default_socket_path))
}

fn display_os(value: OsString) -> String {
    value
        .into_string()
        .unwrap_or_else(|value| value.to_string_lossy().into_owned())
}

fn main() -> eframe::Result {
    let socket_path = match parse_socket_arg() {
        Ok(socket_path) => socket_path,
        Err(err) => {
            eprintln!("waystt-ui: {err}");
            std::process::exit(2);
        }
    };
    let state = Arc::new(Mutex::new(UiState::default()));
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "waystt — live transcript",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(WaysttUiApp::new(
                state,
                socket_path,
                cc.egui_ctx.clone(),
            )))
        }),
    )
}
