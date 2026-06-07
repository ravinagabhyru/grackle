# grackle - Wayland Speech-to-Text Tool

Press a keybind, speak, and get instant text output. A speech-to-text tool that runs as a small Wayland-friendly daemon and is controlled with `grackctl`.

## Features

- **Daemon + control client**: `grackle` + `grackctl` to start/stop/transcribe
- **UNIX philosophy**: Outputs transcribed text to stdout for piping to other tools
- **On-demand transcription**: `grackctl transcribe` records until trailing silence and prints/copies/types the result
- **Audio feedback**: Beeps confirm recording start/stop and success
- **Wayland native**: Works with modern Linux desktops (Hyprland, Niri, etc.)
- **Fully local by default**: Parakeet ONNX models run on-device — no API key, no network
- **Pluggable backends**: Parakeet (local, default), OpenAI Whisper, Google Speech-to-Text, or local Whisper (whisper-rs)

## Requirements

- **Wayland desktop** (Hyprland, Niri, GNOME, KDE, etc.)
- **A transcription backend**: the default Parakeet provider runs locally (just
  download the model — see [Configuration](#configuration)). Only the OpenAI and
  Google providers need an API key / credentials.
- **System packages**:

```bash
# Arch Linux
sudo pacman -S pipewire

# Ubuntu/Debian  
sudo apt install pipewire-pulse

# Fedora
sudo dnf install pipewire-pulseaudio
```

**Optional tools (for output actions):**
```bash
# Arch Linux
sudo pacman -S wl-clipboard wtype ydotool

# Ubuntu/Debian  
sudo apt install wl-clipboard wtype ydotool xclip

# Fedora
sudo dnf install wl-clipboard wtype ydotool xclip

# Setup ydotool permissions and service:
sudo usermod -a -G input $USER

# Enable and start ydotool daemon service
sudo systemctl enable --now ydotool.service

# Set socket environment variable (add to ~/.bashrc or ~/.zshrc)
echo 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket' >> ~/.bashrc

# Log out and back in (or source ~/.bashrc)
```

## Installation

### From AUR (Arch Linux)

```bash
# Using your preferred AUR helper
yay -S grackle-bin
# or
paru -S grackle-bin
```

### Download Binary

1. Download from [GitHub Releases](https://github.com/ravinagabhyru/grackle/releases)
2. Install:

```bash
wget https://github.com/ravinagabhyru/grackle/releases/latest/download/grackle-linux-x86_64
mkdir -p ~/.local/bin
mv grackle-linux-x86_64 ~/.local/bin/grackle
chmod +x ~/.local/bin/grackle

# Add to PATH (add to ~/.bashrc or ~/.zshrc)
export PATH="$HOME/.local/bin:$PATH"
```

## Quick Start

1. **Setup configuration:**
```bash
mkdir -p ~/.config/grackle
cp config.toml.example ~/.config/grackle/config.toml
# Edit ~/.config/grackle/config.toml or export OPENAI_API_KEY for OpenAI.
```

2. **Start the daemon:**
```bash
grackle
```

3. **Control it with `grackctl`:**
```bash
grackctl ping           # prints state/provider/model
grackctl status         # prints state/provider/model
grackctl start          # begin recording
grackctl stop           # stop + transcribe to stdout (default)
grackctl transcribe     # one-shot: auto-start, stop on trailing silence, transcribe

# Copy to clipboard or type directly
grackctl transcribe --output clipboard
grackctl transcribe --output type

# Configure trailing silence (default 3000ms)
grackctl transcribe --silence-ms 5000
```

## Quick Reference

### Common Commands

```bash
# Download a local Whisper GGML model and exit (whisper-rs `local` provider only;
# Parakeet models are downloaded manually — see Configuration)
grackle --download-model

# Start daemon in a terminal
grackle

# One-shot transcription, printing text to stdout
grackctl transcribe

# Copy or type the result
grackctl transcribe --output clipboard
grackctl transcribe --output type

# Start and stop manually
grackctl start
grackctl stop --output type
```

### Keybinding Pattern

Start `grackle` on login, then bind compositor shortcuts to `grackctl`:

```bash
# Start daemon on login (see systemd user unit below)

# Keybind examples
bind = SUPER, R, exec, grackctl start
bind = SUPER SHIFT, R, exec, grackctl stop --output type
bind = SUPER CTRL, R, exec, grackctl stop --output clipboard
```

## Keyboard Shortcuts Setup

### Hyprland

Add to your `~/.config/hypr/hyprland.conf`:

```bash
# grackle - Speech to Text (direct typing)
bind = SUPER, R, exec, grackctl transcribe --output type

# grackle - Speech to Text (clipboard copy)
bind = SUPER SHIFT, R, exec, grackctl transcribe --output clipboard
```

### Niri

Add to your `~/.config/niri/config.kdl`:

```kdl
binds {
    // grackle - Speech to Text (direct typing)
    Mod+R { spawn "grackctl" "transcribe" "--output" "type"; }
    
    // grackle - Speech to Text (clipboard copy)
    Mod+Shift+R { spawn "grackctl" "transcribe" "--output" "clipboard"; }
}
```

**Keybinding Functions:**
- **Super+R** (Hyprland) / **Mod+R** (Niri): Direct typing via ydotool
- **Super+Shift+R** (Hyprland) / **Mod+Shift+R** (Niri): Copy to clipboard

## Usage Examples

Start `grackle` once as a daemon, then use `grackctl` for recording and output actions.

### Basic Usage (stdout)

```bash
# Terminal 1: Start grackle
grackle

# Terminal 2: Record until trailing silence and print the transcript
grackctl transcribe | tee transcription.txt
```

### Output Actions

Use `grackctl` output modes to choose where text goes:

```bash
# Copy transcription to clipboard
grackctl transcribe --output clipboard

# Type transcription directly into focused window
grackctl transcribe --output type

# Save to file with timestamp
printf '%s: %s\n' "$(date)" "$(grackctl transcribe)" >> speech-log.txt
```


## Daemon + grackctl

Run a long-lived daemon and control it with `grackctl`.

### Socket path
- Default: `$XDG_RUNTIME_DIR/grackle/grackle.sock`
- If `XDG_RUNTIME_DIR` is not set, the daemon falls back to `/tmp/grackle-<user>/grackle.sock`.
- You can override the control client path with `grackctl --socket`.

### Commands
- `grackctl ping` → liveness + status summary
- `grackctl status` → state/provider/model
- `grackctl start` → begin recording
- `grackctl stop [--output stdout|clipboard|type]` → stop + transcribe
- `grackctl transcribe [--silence-ms 3000] [--output ...]` → auto-start, stop after trailing silence, transcribe
- `grackctl cancel` → stop without transcription
- `grackctl continuous-start [--silence-ms 700] [--workers 1-4] [--output ...] [--ui]` → stream utterances as they finalize
- `grackctl continuous-stop [--ui]` → flush, stop streaming, emit stats
- `grackctl continuous-status` → report whether continuous mode is active
- `grackctl ui-show` / `ui-hide` / `ui-toggle` → control the live-transcript window

The `--output` modes also accept `--type-newlines spaces|enter|literal` to choose how newlines are rendered when typing.

### Output modes
- `stdout` (default): print text to grackctl stdout
- `clipboard`: copy text using `wl-copy` (fallback: `xclip` on X11)
- `type`: type text into the focused window using `wtype` (fallback: `ydotool`)

Notes:
- Ensure `wl-clipboard` is installed for clipboard operations on Wayland.
- For typing, `wtype` is preferred; `ydotool` may require uinput permissions and a running daemon.

### Systemd user unit (optional)

Create `~/.config/systemd/user/grackle.service`:

```
[Unit]
Description=grackle daemon
After=pipewire.service

[Service]
ExecStart=%h/.local/bin/grackle
Restart=on-failure
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

Then:
```bash
systemctl --user daemon-reload
systemctl --user enable --now grackle.service
```

You can now bind `grackctl` commands in your compositor to control the daemon.

## Configuration

Configuration is read from `~/.config/grackle/config.toml` by default. Override the path with `--config`:

```bash
grackle --config /path/to/custom/config.toml
```

Environment variables always override any value set in the file, so you can keep secrets like API keys out of the file and export them at runtime. The env-var name mirrors the legacy naming: `[section].key` maps to `SECTION_KEY` (e.g. `[openai] api_key` → `OPENAI_API_KEY`, `[llm_refine] api_key` → `LLM_REFINE_API_KEY`).

See [`config.toml.example`](config.toml.example) for the full annotated template.

grackle supports four transcription providers: **Parakeet** (local ONNX, default), **OpenAI Whisper**, **Google Speech-to-Text**, and **local Whisper** (whisper-rs).

### Parakeet / Nemotron ONNX (default, fully local)

Parakeet runs entirely on-device through [`parakeet-rs`](https://github.com/altunenes/parakeet-rs) — no API key, no network at runtime. It is the provider set in the bundled [`config.toml.example`](config.toml.example):

```toml
transcription_provider = "parakeet"

[parakeet]
model_type = "ctc"        # "ctc", "tdt", "eou", or "nemotron"
# model_path = "/path/to/model"   # override the default directory
```

#### Choosing a model type

| `model_type` | Languages | Mode | Best for |
|--------------|-----------|------|----------|
| `ctc`  | English | batch / one-shot | Fast push-to-talk dictation (`grackctl transcribe`) |
| `tdt`  | Multilingual (25 langs) | batch / one-shot | Non-English or mixed-language dictation |
| `eou`  | English | streaming | Continuous mode with lowest latency (model detects end-of-utterance) |
| `nemotron` | English | streaming (experimental) | Continuous mode; grackle's silence detector finalizes utterances |

- `eou` is rejected on the one-shot / stop-and-transcribe path — use `ctc` or `tdt` there.
- `nemotron` requires the ONNX layout from `altunenes/parakeet-rs`, **not** NVIDIA's `.nemo` repository layout.

#### Downloading the models

`grackle --download-model` only fetches Whisper GGML models for the `local`
provider — it does **not** download Parakeet models. Parakeet models are
downloaded manually from Hugging Face into per-type directories under
`~/.local/share/applications/grackle/parakeet/{model_type}/` (or wherever
`[parakeet].model_path` points). The filenames below are what `parakeet-rs`
looks for, so place the files exactly as named.

| `model_type` | Hugging Face source | Files (placed flat in the model dir) |
|--------------|---------------------|--------------------------------------|
| `ctc`  | [`onnx-community/parakeet-ctc-0.6b-ONNX`](https://huggingface.co/onnx-community/parakeet-ctc-0.6b-ONNX) | `model.onnx`, `model.onnx_data`, `tokenizer.json` |
| `tdt`  | [`istupakov/parakeet-tdt-0.6b-v3-onnx`](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx) | `encoder-model.onnx`, `encoder-model.onnx.data`, `decoder_joint-model.onnx`, `vocab.txt` |
| `eou`  | [`altunenes/parakeet-rs`](https://huggingface.co/altunenes/parakeet-rs) → `realtime_eou_120m-v1-onnx/` | `encoder.onnx`, `decoder_joint.onnx`, `tokenizer.json` |
| `nemotron` | [`altunenes/parakeet-rs`](https://huggingface.co/altunenes/parakeet-rs) → `nemotron-speech-streaming-en-0.6b/` | `encoder.onnx`, `encoder.onnx.data`, `decoder_joint.onnx`, `tokenizer.model` |

The models are licensed CC-BY-4.0 by NVIDIA; `parakeet-rs` does not redistribute them.

**Example downloads** (using `curl`; substitute the rows above for other types):

```bash
# ctc (English, one-shot) — note the files live under onnx/ in the repo
DIR=~/.local/share/applications/grackle/parakeet/ctc; mkdir -p "$DIR"
base=https://huggingface.co/onnx-community/parakeet-ctc-0.6b-ONNX/resolve/main
curl -L -o "$DIR/model.onnx"      "$base/onnx/model.onnx"
curl -L -o "$DIR/model.onnx_data" "$base/onnx/model.onnx_data"
curl -L -o "$DIR/tokenizer.json"  "$base/tokenizer.json"

# tdt (multilingual, one-shot)
DIR=~/.local/share/applications/grackle/parakeet/tdt; mkdir -p "$DIR"
base=https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main
curl -L -o "$DIR/encoder-model.onnx"       "$base/encoder-model.onnx"
curl -L -o "$DIR/encoder-model.onnx.data"  "$base/encoder-model.onnx.data"
curl -L -o "$DIR/decoder_joint-model.onnx" "$base/decoder_joint-model.onnx"
curl -L -o "$DIR/vocab.txt"                "$base/vocab.txt"

# eou (English, streaming — recommended for continuous mode)
DIR=~/.local/share/applications/grackle/parakeet/eou; mkdir -p "$DIR"
base=https://huggingface.co/altunenes/parakeet-rs/resolve/main/realtime_eou_120m-v1-onnx
curl -L -o "$DIR/encoder.onnx"       "$base/encoder.onnx"
curl -L -o "$DIR/decoder_joint.onnx" "$base/decoder_joint.onnx"
curl -L -o "$DIR/tokenizer.json"     "$base/tokenizer.json"

# nemotron (English, streaming, experimental)
DIR=~/.local/share/applications/grackle/parakeet/nemotron; mkdir -p "$DIR"
base=https://huggingface.co/altunenes/parakeet-rs/resolve/main/nemotron-speech-streaming-en-0.6b
curl -L -o "$DIR/encoder.onnx"       "$base/encoder.onnx"
curl -L -o "$DIR/encoder.onnx.data"  "$base/encoder.onnx.data"
curl -L -o "$DIR/decoder_joint.onnx" "$base/decoder_joint.onnx"
curl -L -o "$DIR/tokenizer.model"    "$base/tokenizer.model"
```

> The Hugging Face repos also ship quantized variants (`*_int8.onnx`, etc.).
> The CTC loader auto-discovers `model_fp16.onnx` / `model_int8.onnx` / `model_q4.onnx`
> if `model.onnx` is absent, and TDT accepts `*-model.int8.onnx` — handy for
> smaller, faster (slightly less accurate) models on constrained hardware.

### OpenAI Whisper

OpenAI Whisper offers excellent accuracy and supports automatic language detection.

```toml
transcription_provider = "openai"

[openai]
api_key = "sk-..."                 # or export OPENAI_API_KEY

[whisper]
model = "whisper-1"                # default
language = "auto"                  # or "en", "es", ...
timeout_seconds = 60
max_retries = 3
```

### Google Speech-to-Text

Google Speech-to-Text provides fast, accurate transcription with support for many languages and dialects.

**Setup Steps:**

1. **Enable Google Cloud Speech-to-Text API:**
   - Go to [Google Cloud Console](https://console.cloud.google.com/)
   - Create a new project or select existing one
   - Enable the "Cloud Speech-to-Text API"
   - Create a service account and download the JSON key file

2. **Configure grackle for Google:**

```toml
transcription_provider = "google"

[google]
application_credentials = "/path/to/service-account-key.json"
language_code = "en-US"
model = "latest_long"              # or "latest_short"
alternative_languages = ["es-ES", "fr-FR", "de-DE"]   # optional auto-detect
```

### Local Whisper (whisper-rs)

Run transcription locally without sending audio to external APIs. Models are downloaded from [Hugging Face](https://huggingface.co/ggerganov/whisper.cpp) in GGML format.

```toml
transcription_provider = "local"

[whisper]
model = "ggml-base.en.bin"         # stored under ~/.local/share/applications/grackle/models/
```

Download with `grackle --download-model`.

**Available Models (GGML format):**
- `ggml-tiny.bin` - Fastest, least accurate (39 MB)
- `ggml-tiny.en.bin` - English-only tiny model (39 MB)
- `ggml-base.bin` - Small size, good performance (142 MB)
- `ggml-base.en.bin` - English-only base model (142 MB)
- `ggml-small.bin` - Better accuracy than base (466 MB)
- `ggml-small.en.bin` - English-only small model (466 MB)
- `ggml-medium.bin` - Good accuracy/speed balance (1.5 GB)
- `ggml-medium.en.bin` - English-only medium model (1.5 GB)
- `ggml-large.bin` - Best accuracy, slower (2.9 GB)
- `ggml-large-v1.bin` - Large model v1 (2.9 GB)
- `ggml-large-v2.bin` - Large model v2 (2.9 GB)
- `ggml-large-v3.bin` - Latest large model (2.9 GB)

**Recommendations:**
- **For English only**: Use `.en.bin` models for better performance
- **For speed**: `ggml-tiny.en.bin` or `ggml-base.en.bin`
- **For accuracy**: `ggml-large-v3.bin` or `ggml-medium.en.bin`
- **For balance**: `ggml-base.en.bin` (default)

If the configured model is missing, the application will exit with an error.

**Popular Google language codes:**
- `en-US` - English (United States)
- `en-GB` - English (United Kingdom)
- `es-ES` - Spanish (Spain)
- `fr-FR` - French (France)
- `de-DE` - German (Germany)
- `ja-JP` - Japanese
- `zh-CN` - Chinese (Simplified)

### General Settings

**Audio and system settings (apply to all providers):**

```toml
rust_log = "debug"

[beep]
enabled = false                    # disable start/stop beeps
volume = 0.1                       # 0.0 to 1.0
```

### Optional: LLM post-processing

Pipe transcriptions through an LLM to clean up filler words, fix punctuation, and correct misrecognitions before they reach the output. Covers OpenAI, Anthropic, Ollama, and any OpenAI-compatible endpoint via the `genai` crate.

```toml
[llm_refine]
enabled = true
model = "gpt-4o-mini"              # or "claude-haiku-4-5", "llama3.2", ...
# base_url = "http://localhost:11434/v1"   # e.g. Ollama
# api_key = "..."                  # or export LLM_REFINE_API_KEY
timeout_ms = 5000
```

Fails soft — any LLM error logs a warning and the original transcript is emitted.


## Troubleshooting

### IPC (daemon + grackctl)

- Verify the daemon is running and note the socket path it prints on startup (defaults to `$XDG_RUNTIME_DIR/grackle/grackle.sock`).
  - Check the socket exists: `ls -l "$XDG_RUNTIME_DIR/grackle/grackle.sock"`
  - If `XDG_RUNTIME_DIR` is not set, the daemon falls back to `/tmp/grackle-<user>/grackle.sock`. Pass this path to `grackctl` with `--socket`.
- Ensure `grackle` and `grackctl` are using the same socket:
  - `grackctl --socket "$XDG_RUNTIME_DIR/grackle/grackle.sock" ping`
- Remove stale sockets and restart the daemon if needed:
  - `rm -f "$XDG_RUNTIME_DIR/grackle/grackle.sock" && grackle`
- Permissions: the socket directory should be `0700`, socket file `0600`, and both owned by your user.
- Debug logs: run the daemon with `RUST_LOG=debug grackle` and re-run `grackctl`.
- Output actions failing with `no_backend`:
  - Clipboard: install `wl-clipboard` (Wayland) or `xclip` (X11) and re-try.
  - Type: install `wtype` (preferred) or set up `ydotool` (requires input group and running `ydotoold`).

### Audio Issues

If audio recording fails:
- Ensure PipeWire is running: `systemctl --user status pipewire`
- Check microphone permissions
- Verify microphone is not muted


### API Issues

**OpenAI Provider:**
- Verify your OpenAI API key is valid and has sufficient credits
- Check internet connectivity
- Review logs for specific error messages

**Google Provider:**
- Verify your service account JSON file path is correct
- Ensure the Speech-to-Text API is enabled in your Google Cloud project
- Check that your service account has the necessary permissions
- Verify your Google Cloud project has billing enabled
- Review logs for specific error messages

## Development

### Running Tests

```bash
cargo test
```

### Running with Debug Output

```bash
# Using default config location (~/.config/grackle/config.toml)
RUST_LOG=debug cargo run

# Or using a project-local config file for development
RUST_LOG=debug cargo run -- --config config.toml
```

## Building from Source

```bash
git clone https://github.com/ravinagabhyru/grackle.git
cd grackle

# Create config directory and copy example configuration
mkdir -p ~/.config/grackle
cp config.toml.example ~/.config/grackle/config.toml
# Edit ~/.config/grackle/config.toml with your API key (or export OPENAI_API_KEY)

# Build the project
cargo build --release

# Install to local bin
mkdir -p ~/.local/bin
cp ./target/release/grackle ~/.local/bin/
```

## Acknowledgements

**grackle** is a fork of [**waystt**](https://github.com/sevos/waystt) by
[Artur Roszczyk (sevos)](https://github.com/sevos), the original Wayland
speech-to-text daemon this project grew out of. The fork has diverged
significantly — adding a Unix-socket control client, continuous/streaming
transcription, local Parakeet/Nemotron backends, and a live transcript UI — and
has been rebranded as `grackle`. Enormous thanks to Artur for the original work.

Per the GPL, the full upstream history is preserved in this repository and the
original copyright and license are retained. See [NOTICE](NOTICE) for details.

## License

Licensed under GPL v3.0 or later. Source code: https://github.com/ravinagabhyru/grackle

This project is a fork of waystt (© Artur Roszczyk), also GPL-3.0-or-later;
the original license and copyright are retained. See [LICENSE](LICENSE) for full
terms and [NOTICE](NOTICE) for attribution.
