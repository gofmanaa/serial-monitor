# Serial Monitor

A Rust-based terminal user interface (TUI) application for interacting with Arduino or other serial devices. It provides real-time monitoring of serial output, command input with history, and optional logging to a file with human-readable timestamps.

## Features

- **Interactive TUI**: Displays serial output in a scrollable pane with color-coded messages (green for normal, red for errors containing "ERROR").
- **Command Input**: Send commands to the serial device with cursor navigation, backspace, and command history (Up/Down arrow keys).
- **Line Wrapping**: Long serial output lines wrap within the TUI for readability.
- **Memory Cap**: Limits in-memory log lines to 1000 to prevent unbounded memory usage.
- **File Logging**: Logs serial output and user commands to a file with RFC 3339 timestamps (e.g., `2025-11-10T11:06:00+00:00`).
- **Configurable CLI**: Customize serial port, baud rate, log file path, and disable logging via command-line arguments.
- **Input Validation**: Ensures valid serial port paths (e.g., `/dev/tty*` or `COM*`) and common baud rates (300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200).
- **Error Handling**: Gracefully handles serial and file I/O errors, with warnings for potentially inaccessible ports.

## Requirements

- Rust (latest stable version recommended, e.g., 1.82 or later)
- Cargo (included with Rust)
- A serial device (e.g., Arduino connected via USB)
- Unix-like system (Linux/macOS) or Windows

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/serial-monitor.git
   cd serial-monitor
   
2. Build the project:
```bash
cargo build --release
```

## Usage
Run the application with default settings (port: /dev/ttyUSB0, baud rate: 57600, log file: serial_monitor.log):
```bash
cargo run --release -- [OPTIONS]
```

| Option | Description | Default Value | Required |
|:-------|:------------|:--------------|:---------|
| `--port <PORT>` | Serial port name (e.g., `/dev/ttyUSB0` or `COM1`) | `/dev/ttyUSB0` | No |
| `--baud-rate <BAUD_RATE>` | Baud rate (300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200) | `57600` | No |
| `--log-file <LOG_FILE>` | Log file path | `serial_monitor.log` | No |
| `--no-log` | Disable logging to file | Logging enabled | No |

```bash
# Use a different port and baud rate
cargo run --release -- --port /dev/ttyACM0 --baud-rate 115200

# Disable logging
cargo run --release -- --no-log

# Custom log file
cargo run --release -- --log-file my_log.log
```

## Keybindings
| Key | Action |
|:-------|:------------|
| Esc | Exit the application|
| Enter | Send the current input as a command to the serial device|
| Up | Navigate to previous command in history|
| Down | Navigate to next command in history or clear input|
| Left/Right | Move cursor in input field|
| Backspace | Delete character before cursor|
| PageUp | Scroll up in the output pane|
| PageDown | Scroll down in the output pane|
| Any character | Insert character into input field|