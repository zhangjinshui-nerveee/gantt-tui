# gantt-tui

A terminal user interface (TUI) application for visualizing and managing Gantt charts directly from your command line. This tool aims to provide a lightweight yet powerful way to keep track of project timelines and task dependencies without leaving your terminal.

![gantt_tui](https://github.com/user-attachments/assets/8da888dc-7225-4ffb-a8e2-e09bd414bb67)

## Features

*   **Interactive Gantt Chart Display**: View tasks and their durations in a clear, interactive timeline.
*   **Task Management**: Add, edit, and delete tasks directly within the TUI.
*   **Dependency Tracking**: Define and visualize dependencies between tasks.
*   **Customizable Views**: Filter and sort tasks to focus on what matters most.
*   **Keyboard-driven Interface**: Efficient navigation and interaction using only your keyboard.

## Installation

To build and run `gantt-tui`, you will need to have [Rust](https://www.rust-lang.org/tools/install) and Cargo installed on your system.

1.  **Clone the repository**:
    ```bash
    git clone https://github.com/your-username/gantt-tui.git
    cd gantt-tui
    ```
    (Note: Replace `https://github.com/your-username/gantt-tui.git` with the actual repository URL if it's hosted elsewhere.)

2.  **Build the application**:
    ```bash
    cargo build --release
    ```
    This command compiles the application and places the executable in the `target/release/` directory.

## Usage

Once built, you can run the application from the project root:

```bash
./target/release/gantt-tui
```

Alternatively, if you have added `~/.cargo/bin` to your PATH, you can install it and run directly:

```bash
cargo install --path .
gantt-tui
```

(Further usage instructions, including command-line arguments and keybindings, will be added here as the application develops.)

## Contributing

Contributions are welcome! Please see the `CONTRIBUTING.md` file (to be created) for more details on how to contribute.

## License

This project is licensed under the [MIT License](https://opensource.org/licenses/MIT).
