# Clockify Rust TUI Development Summary & Handoff

Welcome to the Clockify Rust TUI project! This document outlines what we set out to build, what has been achieved, how to run/use the application, and key context for developers or AI agents resuming work on this codebase on another machine.

---

## 🎯 Project Vision & Goals

The goal of this project is to build a high-performance, visually rich Terminal User Interface (TUI) client for **Clockify** in Rust. The application connects to the Clockify API to retrieve time logs, display workspaces and projects, start/stop timers, and manually log time entries.

### Key Requirements
*   **Rust Implementation:** Modern, type-safe code using asynchronous tokio runtime and reqwest.
*   **Rich Aesthetics:** Use clean, rounded box structures (`BorderType::Rounded` and `BorderType::Thick` for active elements) and maintain terminal default transparency (no forced background colors).
*   **Intuitive Panels:** Vertical left sidebar showing navigation views, workspaces, projects, and active timer stats, with the main content tab on the right.
*   **Unified Sidebar Navigation:** Seamless vertical list navigation through all sidebar sections via arrow keys, using `Tab` to toggle focus between the sidebar panel and the main calendar panel.
*   **Prominent Calendar Focus:** Thick borders and selection cursor icons (`▶`) in day grid views to show exactly which day is hovered and whether the calendar is focused.
*   **Project Filtering:** Local filtering on selected projects in all views (Day, Week, Month, Dashboard) with clear visual feedback in the header title showing the active filter.

---

## 🛠️ Current Tech Stack & Crates

*   **TUI Engine:** `ratatui` (v0.26)
*   **Terminal Interface:** `crossterm` (v0.27)
*   **Asynchronous Client:** `tokio` (v1.37)
*   **REST Requests:** `reqwest` (v0.12)
*   **JSON Serialization:** `serde` / `serde_json`
*   **Configuration Management:** `toml` and `dirs` (stores keys in `~/Library/Application Support/clockify-tui/config.toml` on macOS)
*   **Time Utilities:** `chrono`

---

## 📂 Core Project Structure

*   [src/main.rs](file:///Users/lagos/code/clockify/src/main.rs): Entry point. Handles command line parsing (`clap`) for authenticating tokens (`clockify auth --token <API_KEY>`) and launches the TUI runner.
*   [src/config.rs](file:///Users/lagos/code/clockify/src/config.rs): Decouples reading and writing local configuration files.
*   [src/api/mod.rs](file:///Users/lagos/code/clockify/src/api/mod.rs): Async HTTP client interacting with the Clockify REST API endpoints.
*   [src/api/models.rs](file:///Users/lagos/code/clockify/src/api/models.rs): Rust representations of Clockify structs (User, Workspace, Project, TimeEntry).
*   [src/tui/mod.rs](file:///Users/lagos/code/clockify/src/tui/mod.rs): Main application loop, layout definitions, rendering buffers, key events, and helper calculations.

---

## ✅ Features Implemented & Done

### 1. Unified Two-Panel Navigation & Focus
*   Pressing **`Tab`** toggles focus back and forth between the **Left Panel (Sidebar)** and the **Right Panel (Main Content)**.
*   When focus is in the Left Panel, **`Up` / `Down` Arrow keys** navigate through all Views, Workspaces, and Projects vertically as one single continuous selection.
*   Only the active sub-panel in the sidebar displays a **Yellow** highlight border. The currently hovered list item has reversed background/foreground colors, while other selection fields are highlighted in a subtle **Cyan** text color.

### 2. High-Contrast Day Selection & Hovering
*   When navigating Month or Week calendars, the selected cell displays a **Thick Border** (`BorderType::Thick`) if the calendar panel is focused, and a **Rounded Border** if it is unfocused.
*   A selection cursor **`▶`** prepends the hovered cell day number (Month view) or day header (Week view) for absolute clarity.
*   Cell typography displays day names/numbers (white/cyan) and daily hours (bold green/gray) independently.
*   **`Left` / `Right` Arrow keys** move selection day-by-day in focused calendar cells.

### 3. Smart Project Filtering & Views
*   Selecting a Project in the Projects sidebar box filters all logged hours and items locally in memory.
*   **Header Titles:** Title headers dynamically append a `[Project: <project_name>]` suffix when filtering is active so you never lose track.
*   Project selector defaults to `[All Projects]` on startup.

### 4. Active Timer Widget
*   Includes a real-time updating box in the sidebar showing status of any active running trackers:
    *   If running: Shows `● RUNNING  01:23:45` (live ticking elapsed duration), entry description, and project name.
    *   If idle: Shows `○ IDLE  No active timer  Press [s] to start`.

### 5. Manual Time Entry Modal
*   Pressing `n` opens a centered popup card with Description, Start Time (`HH:MM`), and End Time (`HH:MM`).
*   Auto-fills with current time and automatically logs it under the currently selected project and selected date when you hit `Enter`.

---

## ⌨️ Keyboard Shortcut Quick Reference

| Key | Context | Action |
|---|---|---|
| **`Tab`** | Navigation | Toggle focus between Sidebar and Main Calendar Panel |
| **`Up` / `Down`** | Sidebar Active | Move focus selection vertically across all options |
| **`Left` / `Right`** | Sidebar Active | Shift target date by 1 block (1 day for Day View, 1 week for Week, 1 month for Month) |
| **`Up` / `Down`** | Calendar Active | Move calendar selection week-by-week (7 days) or day-by-day |
| **`Left` / `Right`** | Calendar Active | Move calendar selection day-by-day |
| **`t`** | Any | Reset target date to Today |
| **`s`** | Any | Toggle running timer (uses selected project) |
| **`n`** | Any | Open Manual Time Entry Modal popup |
| **`Enter`** | Modal Open | Save manual log and close modal |
| **`Esc`** | Modal Open | Cancel manual log and close modal |
| **`Esc` / `q`** | App Running | Safely quit Clockify TUI |

---

## 🤖 Instructions for the Next Agent (Desktop Machine Handoff)

If you are an Antigravity agent or another developer starting on this repository:
1.  **Dependencies & Compilation:** Run `cargo check` and `cargo build` to ensure the toolchain is healthy.
2.  **API Credentials Setup:** The TUI relies on a valid Clockify API token. Run:
    ```bash
    cargo run -- auth --token <YOUR_CLOCKIFY_API_TOKEN>
    ```
    This stores the token in your platform-specific config directory (`~/Library/Application Support/clockify-tui/config.toml` on macOS).
3.  **TUI Entry Point:** Run the app using:
    ```bash
    cargo run
    ```
4.  **Verifying Local Code Changes:** Review code inside `src/tui/mod.rs` to inspect the layout split rules, navigation keys mapping under `loop`, and custom styling variables.
