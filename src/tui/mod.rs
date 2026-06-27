use std::io;
use std::sync::Arc;
use std::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, List, ListItem, ListState, Clear, BorderType},
    Frame, Terminal,
};
use chrono::{Datelike, NaiveDate};

use crate::api::Client;
use crate::api::models::{Project, TimeEntry, User, Workspace};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ActiveTab {
    Day,
    Week,
    Month,
    Dashboard,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Focus {
    Sidebar,
    Calendar,
}

pub struct AppState {
    pub api_client: Arc<Client>,
    pub user: Option<User>,
    pub workspaces: Vec<Workspace>,
    pub projects: Vec<Project>,
    pub time_entries: Vec<TimeEntry>,
    pub selected_workspace_id: Option<String>,
    pub selected_project_id: Option<String>,
    pub selected_date: NaiveDate,
    pub active_tab: ActiveTab,
    pub focus: Focus,
    
    // Lists states
    pub sidebar_index: usize,
    pub sidebar_selection: usize,
    pub workspace_list_state: ListState,
    pub project_list_state: ListState,
    
    pub error: Option<String>,
    pub loading: bool,
    pub status_message: Option<String>,

    // Modal fields
    pub show_input_modal: bool,
    pub active_modal_field: usize, // 0 = Description, 1 = Start Time, 2 = End Time
    pub modal_description: String,
    pub modal_start_time: String,
    pub modal_end_time: String,
}

pub enum Action {
    SetUser(User),
    SetWorkspaces(Vec<Workspace>),
    SetProjects(Vec<Project>),
    SetTimeEntries(Vec<TimeEntry>),
    SetError(String),
    SetStatus(String),
    Loading(bool),
    RefreshLogs,
}

fn get_total_sidebar_items(state: &AppState) -> usize {
    4 + state.workspaces.len() + state.projects.len() + 1
}

fn update_selections_from_sidebar_index(state: &mut AppState, tx: &tokio::sync::mpsc::Sender<Action>) {
    let ws_len = state.workspaces.len();
    let sel = state.sidebar_selection;

    if sel < 4 {
        state.sidebar_index = sel;
        state.active_tab = match sel {
            0 => ActiveTab::Day,
            1 => ActiveTab::Week,
            2 => ActiveTab::Month,
            _ => ActiveTab::Dashboard,
        };
    } else if sel < 4 + ws_len {
        let ws_idx = sel - 4;
        state.workspace_list_state.select(Some(ws_idx));
        if let Some(ws) = state.workspaces.get(ws_idx) {
            if Some(ws.id.clone()) != state.selected_workspace_id {
                state.selected_workspace_id = Some(ws.id.clone());
                let tx_clone = tx.clone();
                let client_clone = state.api_client.clone();
                let ws_id = ws.id.clone();
                tokio::spawn(async move {
                    fetch_workspace_details(client_clone, ws_id, tx_clone).await;
                });
            }
        }
    } else {
        let proj_idx = sel - (4 + ws_len);
        let max_proj_idx = state.projects.len();
        let proj_idx = std::cmp::min(proj_idx, max_proj_idx);
        state.project_list_state.select(Some(proj_idx));
        if proj_idx == 0 {
            state.selected_project_id = None;
        } else if let Some(p) = state.projects.get(proj_idx - 1) {
            state.selected_project_id = Some(p.id.clone());
        }
    }
}

pub fn run(api_key: String) {
    // Setup terminal
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    // Start tokio runtime for background tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let client = Arc::new(Client::new(api_key));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Action>(100);

    // Initial state
    let mut state = AppState {
        api_client: client.clone(),
        user: None,
        workspaces: Vec::new(),
        projects: Vec::new(),
        time_entries: Vec::new(),
        selected_workspace_id: None,
        selected_project_id: None,
        selected_date: chrono::Local::now().date_naive(),
        active_tab: ActiveTab::Day,
        focus: Focus::Sidebar,
        sidebar_index: 0,
        sidebar_selection: 0,
        workspace_list_state: ListState::default(),
        project_list_state: ListState::default(),
        error: None,
        loading: true,
        status_message: Some("Connecting to Clockify...".to_string()),
        
        // Modal fields
        show_input_modal: false,
        active_modal_field: 0,
        modal_description: String::new(),
        modal_start_time: "09:00".to_string(),
        modal_end_time: "10:00".to_string(),
    };

    // Load initial data
    let tx_clone = tx.clone();
    let client_clone = client.clone();
    tokio::spawn(async move {
        let _ = tx_clone.send(Action::Loading(true)).await;
        
        let user_res = client_clone.get_user().await;
        let ws_res = client_clone.get_workspaces().await;

        match (user_res, ws_res) {
            (Ok(user), Ok(workspaces)) => {
                let default_ws = user.default_workspace.clone();
                let _ = tx_clone.send(Action::SetUser(user)).await;
                let _ = tx_clone.send(Action::SetWorkspaces(workspaces)).await;
                let _ = tx_clone.send(Action::Loading(false)).await;
                let _ = tx_clone.send(Action::SetStatus(format!("Loaded user profile successfully!"))).await;
                
                // Fetch projects and entries for workspace
                fetch_workspace_details(client_clone, default_ws, tx_clone).await;
            }
            (Err(e), _) | (_, Err(e)) => {
                let _ = tx_clone.send(Action::SetError(format!("Failed to connect: {}", e))).await;
                let _ = tx_clone.send(Action::Loading(false)).await;
            }
        }
    });

    // Main event loop
    loop {
        // Draw layout
        terminal.draw(|f| {
            draw_ui(f, &mut state);
        }).unwrap();

        // Handle events
        if event::poll(Duration::from_millis(50)).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                if key.kind == KeyEventKind::Press {
                    if state.show_input_modal {
                        match key.code {
                            KeyCode::Esc => {
                                state.show_input_modal = false;
                            }
                            KeyCode::Tab => {
                                state.active_modal_field = (state.active_modal_field + 1) % 3;
                            }
                            KeyCode::Down => {
                                state.active_modal_field = (state.active_modal_field + 1) % 3;
                            }
                            KeyCode::Up => {
                                state.active_modal_field = (state.active_modal_field + 2) % 3;
                            }
                            KeyCode::Backspace => {
                                match state.active_modal_field {
                                    0 => { state.modal_description.pop(); }
                                    1 => { state.modal_start_time.pop(); }
                                    2 => { state.modal_end_time.pop(); }
                                    _ => {}
                                }
                            }
                            KeyCode::Char(c) => {
                                match state.active_modal_field {
                                    0 => { state.modal_description.push(c); }
                                    1 => { if state.modal_start_time.len() < 5 { state.modal_start_time.push(c); } }
                                    2 => { if state.modal_end_time.len() < 5 { state.modal_end_time.push(c); } }
                                    _ => {}
                                }
                            }
                            KeyCode::Enter => {
                                if let Some(ws_id) = &state.selected_workspace_id {
                                    let tx_clone = tx.clone();
                                    let client_clone = state.api_client.clone();
                                    let ws_id_clone = ws_id.clone();
                                    let proj_id = state.selected_project_id.clone();
                                    let desc = state.modal_description.clone();
                                    let start_time_str = state.modal_start_time.clone();
                                    let end_time_str = state.modal_end_time.clone();
                                    let sel_date = state.selected_date;
                                    
                                    let parse_time = |t_str: &str| -> Option<(u32, u32)> {
                                        let parts: Vec<&str> = t_str.split(':').collect();
                                        if parts.len() == 2 {
                                            let h = parts[0].parse::<u32>().ok()?;
                                            let m = parts[1].parse::<u32>().ok()?;
                                            if h < 24 && m < 60 {
                                                return Some((h, m));
                                            }
                                        }
                                        None
                                    };
                                    
                                    if let (Some((s_h, s_m)), Some((e_h, e_m))) = (parse_time(&start_time_str), parse_time(&end_time_str)) {
                                        if let (Some(s_dt), Some(e_dt)) = (
                                            sel_date.and_hms_opt(s_h, s_m, 0),
                                            sel_date.and_hms_opt(e_h, e_m, 0)
                                        ) {
                                            let start_rfc = s_dt.and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                                            let end_rfc = e_dt.and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                                            
                                            tokio::spawn(async move {
                                                let _ = tx_clone.send(Action::Loading(true)).await;
                                                match client_clone.create_time_entry(&ws_id_clone, proj_id.as_deref(), &desc, &start_rfc, &end_rfc).await {
                                                    Ok(_) => {
                                                        let _ = tx_clone.send(Action::SetStatus("Time Entry Saved!".to_string())).await;
                                                        let _ = tx_clone.send(Action::RefreshLogs).await;
                                                    }
                                                    Err(e) => {
                                                        let _ = tx_clone.send(Action::SetError(format!("Error saving time entry: {}", e))).await;
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                                state.show_input_modal = false;
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Esc => break,
                            KeyCode::Char('n') => {
                                state.show_input_modal = true;
                                state.active_modal_field = 0;
                                state.modal_description = String::new();
                                let now = chrono::Local::now();
                                state.modal_start_time = now.format("%H:%M").to_string();
                                state.modal_end_time = (now + chrono::Duration::hours(1)).format("%H:%M").to_string();
                            }
                            KeyCode::Tab => {
                                // Cycle focus between Sidebar and Calendar
                                state.focus = match state.focus {
                                    Focus::Sidebar => Focus::Calendar,
                                    Focus::Calendar => Focus::Sidebar,
                                };
                            }
                        KeyCode::Char('t') => {
                            state.selected_date = chrono::Local::now().date_naive();
                            state.status_message = Some("Jumped to Today".to_string());
                            let _ = tx.try_send(Action::RefreshLogs);
                        }
                        KeyCode::Left => {
                            if state.focus == Focus::Calendar {
                                state.selected_date = state.selected_date - chrono::Duration::days(1);
                            } else {
                                // Go back based on view
                                state.selected_date = match state.active_tab {
                                    ActiveTab::Day => state.selected_date - chrono::Duration::days(1),
                                    ActiveTab::Week => state.selected_date - chrono::Duration::days(7),
                                    ActiveTab::Month => {
                                        let prev_month = if state.selected_date.month() == 1 { 12 } else { state.selected_date.month() - 1 };
                                        let prev_year = if state.selected_date.month() == 1 { state.selected_date.year() - 1 } else { state.selected_date.year() };
                                        NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap()
                                    }
                                    ActiveTab::Dashboard => state.selected_date - chrono::Duration::days(1),
                                };
                            }
                            let _ = tx.try_send(Action::RefreshLogs);
                        }
                        KeyCode::Right => {
                            if state.focus == Focus::Calendar {
                                state.selected_date = state.selected_date + chrono::Duration::days(1);
                            } else {
                                // Go forward based on view
                                state.selected_date = match state.active_tab {
                                    ActiveTab::Day => state.selected_date + chrono::Duration::days(1),
                                    ActiveTab::Week => state.selected_date + chrono::Duration::days(7),
                                    ActiveTab::Month => {
                                        let next_month = if state.selected_date.month() == 12 { 1 } else { state.selected_date.month() + 1 };
                                        let next_year = if state.selected_date.month() == 12 { state.selected_date.year() + 1 } else { state.selected_date.year() };
                                        NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap()
                                    }
                                    ActiveTab::Dashboard => state.selected_date + chrono::Duration::days(1),
                                };
                            }
                            let _ = tx.try_send(Action::RefreshLogs);
                        }
                        KeyCode::Up => {
                            match state.focus {
                                Focus::Sidebar => {
                                    let total = get_total_sidebar_items(&state);
                                    if state.sidebar_selection > 0 {
                                        state.sidebar_selection -= 1;
                                    } else {
                                        state.sidebar_selection = total.saturating_sub(1);
                                    }
                                    update_selections_from_sidebar_index(&mut state, &tx);
                                }
                                Focus::Calendar => {
                                    match state.active_tab {
                                        ActiveTab::Month | ActiveTab::Week => {
                                            state.selected_date = state.selected_date - chrono::Duration::days(7);
                                        }
                                        _ => {
                                            state.selected_date = state.selected_date - chrono::Duration::days(1);
                                        }
                                    }
                                    let _ = tx.try_send(Action::RefreshLogs);
                                }
                            }
                        }
                        KeyCode::Down => {
                            match state.focus {
                                Focus::Sidebar => {
                                    let total = get_total_sidebar_items(&state);
                                    if total > 0 {
                                        state.sidebar_selection = (state.sidebar_selection + 1) % total;
                                    }
                                    update_selections_from_sidebar_index(&mut state, &tx);
                                }
                                Focus::Calendar => {
                                    match state.active_tab {
                                        ActiveTab::Month | ActiveTab::Week => {
                                            state.selected_date = state.selected_date + chrono::Duration::days(7);
                                        }
                                        _ => {
                                            state.selected_date = state.selected_date + chrono::Duration::days(1);
                                        }
                                    }
                                    let _ = tx.try_send(Action::RefreshLogs);
                                }
                            }
                        }
                        KeyCode::Char('s') => {
                            // Toggle timer
                            if let Some(ws_id) = &state.selected_workspace_id {
                                let running = state.time_entries.iter().find(|e| e.time_interval.end.is_none());
                                let tx_clone = tx.clone();
                                let client_clone = state.api_client.clone();
                                let ws_id_clone = ws_id.clone();
                                
                                if let Some(_timer) = running {
                                    // Stop timer
                                    let user_id = state.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                                    tokio::spawn(async move {
                                        let _ = tx_clone.send(Action::Loading(true)).await;
                                        match client_clone.stop_time_entry(&ws_id_clone, &user_id).await {
                                            Ok(_) => {
                                                let _ = tx_clone.send(Action::SetStatus("Timer Stopped!".to_string())).await;
                                                let _ = tx_clone.send(Action::RefreshLogs).await;
                                            }
                                            Err(e) => {
                                                let _ = tx_clone.send(Action::SetError(format!("Error stopping timer: {}", e))).await;
                                            }
                                        }
                                    });
                                } else {
                                    // Start timer
                                    let proj_id = state.selected_project_id.clone();
                                    tokio::spawn(async move {
                                        let _ = tx_clone.send(Action::Loading(true)).await;
                                        match client_clone.start_time_entry(&ws_id_clone, proj_id.as_deref(), "Tracked from Clockify TUI").await {
                                            Ok(_) => {
                                                let _ = tx_clone.send(Action::SetStatus("Timer Started!".to_string())).await;
                                                let _ = tx_clone.send(Action::RefreshLogs).await;
                                            }
                                            Err(e) => {
                                                let _ = tx_clone.send(Action::SetError(format!("Error starting timer: {}", e))).await;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

        // Process background actions
        while let Ok(action) = rx.try_recv() {
            match action {
                Action::SetUser(u) => {
                    state.user = Some(u.clone());
                    if state.selected_workspace_id.is_none() {
                        state.selected_workspace_id = Some(u.default_workspace);
                    }
                }
                Action::SetWorkspaces(w) => {
                    state.workspaces = w.clone();
                    // Select active in list
                    if let Some(ws_id) = &state.selected_workspace_id {
                        if let Some(idx) = w.iter().position(|ws| &ws.id == ws_id) {
                            state.workspace_list_state.select(Some(idx));
                        }
                    }
                }
                Action::SetProjects(p) => {
                    state.projects = p.clone();
                    state.project_list_state.select(Some(0));
                    state.selected_project_id = None;
                }
                Action::SetTimeEntries(t) => {
                    state.time_entries = t;
                    state.loading = false;
                }
                Action::SetError(err) => {
                    state.error = Some(err);
                    state.loading = false;
                }
                Action::SetStatus(msg) => {
                    state.status_message = Some(msg);
                }
                Action::Loading(l) => {
                    state.loading = l;
                }
                Action::RefreshLogs => {
                    if let Some(ws_id) = &state.selected_workspace_id {
                        let tx_clone = tx.clone();
                        let client_clone = state.api_client.clone();
                        let ws_id_clone = ws_id.clone();
                        let user_id = state.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                        
                        // We fetch logs surrounding the selected date
                        let end_date = state.selected_date + chrono::Duration::days(31);
                        let start_date = state.selected_date - chrono::Duration::days(31);
                        let start_str = start_date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        let end_str = end_date.and_hms_opt(23, 59, 59).unwrap().and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

                        tokio::spawn(async move {
                            if let Ok(logs) = client_clone.get_time_entries(&ws_id_clone, &user_id, &start_str, &end_str).await {
                                let _ = tx_clone.send(Action::SetTimeEntries(logs)).await;
                            }
                        });
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    ).unwrap();
    terminal.show_cursor().unwrap();
}

async fn fetch_workspace_details(client: Arc<Client>, workspace_id: String, tx: tokio::sync::mpsc::Sender<Action>) {
    let _ = tx.send(Action::Loading(true)).await;
    
    // Fetch projects
    let proj_res = client.get_projects(&workspace_id).await;
    
    // Fetch time entries (last 30 days and next 30 days to populate views)
    let user_res = client.get_user().await;
    let user_id = user_res.map(|u| u.id).unwrap_or_default();
    
    let end_date = chrono::Local::now().date_naive() + chrono::Duration::days(31);
    let start_date = chrono::Local::now().date_naive() - chrono::Duration::days(31);
    let start_str = start_date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let end_str = end_date.and_hms_opt(23, 59, 59).unwrap().and_local_timezone(chrono::Local).unwrap().to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    
    let logs_res = client.get_time_entries(&workspace_id, &user_id, &start_str, &end_str).await;

    match (proj_res, logs_res) {
        (Ok(projects), Ok(logs)) => {
            let _ = tx.send(Action::SetProjects(projects)).await;
            let _ = tx.send(Action::SetTimeEntries(logs)).await;
            let _ = tx.send(Action::SetStatus("Data synced with Clockify".to_string())).await;
        }
        (Err(e), _) | (_, Err(e)) => {
            let _ = tx.send(Action::SetError(format!("Failed to load workspace data: {}", e))).await;
        }
    }
}

// -------------------------------------------------------------
// UI DRAWING FUNCTIONS
// -------------------------------------------------------------

fn draw_ui(f: &mut Frame, state: &mut AppState) {
    // Transparent style - no background color
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Sidebar + Main view
            Constraint::Length(3), // Footer / Status Bar
        ])
        .split(f.size());

    draw_header(f, main_layout[0], state);
    draw_footer(f, main_layout[2], state);

    // Inner layout: Sidebar + Main view
    let inner_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar
            Constraint::Percentage(75), // Main content
        ])
        .split(main_layout[1]);

    draw_sidebar(f, inner_layout[0], state);
    draw_main_panel(f, inner_layout[1], state);

    if state.show_input_modal {
        draw_input_modal(f, state);
    }
}

fn draw_header(f: &mut Frame, rect: Rect, state: &AppState) {
    let ws_name = state.selected_workspace_id.as_ref()
        .and_then(|id| state.workspaces.iter().find(|ws| &ws.id == id).map(|ws| ws.name.as_str()))
        .unwrap_or("No Workspace selected");

    let running_timer = state.time_entries.iter().find(|e| e.time_interval.end.is_none());
    
    let timer_text = match running_timer {
        Some(t) => {
            let desc = t.description.as_deref().unwrap_or("No description");
            format!(" ● Tracking: {}", desc)
        }
        None => " ○ Idle".to_string(),
    };

    let title = format!(
        " Clockify TUI  |  Workspace: {}  |  Status:{}",
        ws_name, timer_text
    );

    let header_widget = Paragraph::new(title)
        .block(Block::default().borders(Borders::BOTTOM))
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    f.render_widget(header_widget, rect);
}

fn draw_footer(f: &mut Frame, rect: Rect, state: &AppState) {
    let loading_indicator = if state.loading { " [LOADING...] " } else { "" };
    
    let status_text = match &state.error {
        Some(err) => format!("Error: {}", err),
        None => state.status_message.clone().unwrap_or_else(|| "System Ready".to_string()),
    };

    let footer_text = format!(
        "{} | Keys: [Tab] Focus Panel | [Arrows] Navigate | [t] Today | [n] New Entry | [s] Start/Stop Timer | [Esc/q] Quit {}",
        status_text, loading_indicator
    );

    let footer_widget = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer_widget, rect);
}

fn draw_sidebar(f: &mut Frame, rect: Rect, state: &mut AppState) {
    let sidebar_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // View options
            Constraint::Length(3), // Workspace selection (made extremely compact)
            Constraint::Min(6),    // Project selection
            Constraint::Length(5), // Active Timer box
            Constraint::Length(5), // Quick stats / Selection info box
        ])
        .split(rect);

    let is_views_focused = state.focus == Focus::Sidebar && state.sidebar_selection < 4;
    let is_ws_focused = state.focus == Focus::Sidebar 
        && state.sidebar_selection >= 4 
        && state.sidebar_selection < 4 + state.workspaces.len();
    let is_proj_focused = state.focus == Focus::Sidebar 
        && state.sidebar_selection >= 4 + state.workspaces.len();

    // 1. Navigation items list
    let mut nav_items = Vec::new();
    let tabs = [ActiveTab::Day, ActiveTab::Week, ActiveTab::Month, ActiveTab::Dashboard];
    let tab_names = ["Day View", "Week View", "Month View", "Dashboard"];
    
    for i in 0..4 {
        let is_selected_tab = state.active_tab == tabs[i];
        let is_current_hover = is_views_focused && state.sidebar_selection == i;
        
        let prefix = if is_selected_tab { "▶ " } else { "  " };
        let mut style = Style::default();
        if is_current_hover {
            style = style.add_modifier(Modifier::REVERSED);
        } else if is_selected_tab {
            style = style.fg(Color::Cyan);
        }
        
        nav_items.push(ListItem::new(Span::styled(format!("{}{}", prefix, tab_names[i]), style)));
    }

    let nav_border_style = if is_views_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let nav_list = List::new(nav_items)
        .block(Block::default().borders(Borders::ALL).title("Views").border_style(nav_border_style).border_type(BorderType::Rounded));

    f.render_widget(nav_list, sidebar_layout[0]);

    // 2. Workspace Selector
    let ws_items: Vec<ListItem> = state.workspaces.iter()
        .map(|ws| ListItem::new(ws.name.clone()))
        .collect();

    let ws_border_style = if is_ws_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let ws_highlight_style = if is_ws_focused {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let ws_list = List::new(ws_items)
        .block(Block::default().borders(Borders::ALL).title("Workspaces").border_style(ws_border_style).border_type(BorderType::Rounded))
        .highlight_style(ws_highlight_style);

    f.render_stateful_widget(ws_list, sidebar_layout[1], &mut state.workspace_list_state);

    // 3. Project Selector
    let mut proj_items = vec![ListItem::new("[All Projects]")];
    for p in &state.projects {
        proj_items.push(ListItem::new(p.name.clone()));
    }

    let proj_border_style = if is_proj_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let proj_highlight_style = if is_proj_focused {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let proj_list = List::new(proj_items)
        .block(Block::default().borders(Borders::ALL).title("Projects").border_style(proj_border_style).border_type(BorderType::Rounded))
        .highlight_style(proj_highlight_style);

    f.render_stateful_widget(proj_list, sidebar_layout[2], &mut state.project_list_state);

    // 4. Active Timer Box
    let running_timer = state.time_entries.iter().find(|e| e.time_interval.end.is_none());
    let (timer_text, timer_border_style) = match running_timer {
        Some(t) => {
            let desc = truncate_str(t.description.as_deref().unwrap_or("(No description)"), 22);
            let elapsed = get_elapsed_time_string(&t.time_interval.start);
            let proj_name = if let Some(p_id) = &t.project_id {
                state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown Project")
            } else {
                "No Project"
            };
            let proj_truncated = truncate_str(proj_name, 22);
            (
                format!(" ● RUNNING  {}\n {}\n [{}]", elapsed, desc, proj_truncated),
                Style::default().fg(Color::Green)
            )
        }
        None => (
            " ○ IDLE\n No active timer\n Press [s] to start".to_string(),
            Style::default().fg(Color::DarkGray)
        )
    };

    let timer_block = Paragraph::new(timer_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" Active Timer ")
            .border_style(timer_border_style)
            .border_type(BorderType::Rounded)
        );
    f.render_widget(timer_block, sidebar_layout[3]);

    // 5. Quick Info Block
    let today = chrono::Local::now().date_naive();
    let mut today_min = 0;
    for entry in &state.time_entries {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&entry.time_interval.start) {
            if dt.with_timezone(&chrono::Local).date_naive() == today {
                if let Some(dur) = &entry.time_interval.duration {
                    today_min += parse_iso_duration(dur);
                }
            }
        }
    }
    let today_hours = today_min as f64 / 60.0;

    let selected_proj_name = match &state.selected_project_id {
        Some(p_id) => state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown Project"),
        None => "All Projects",
    };

    let info_text = format!(
        " Project:  {}\n Date:     {}\n Today:    {:.1} hrs",
        truncate_str(selected_proj_name, 20),
        state.selected_date.format("%Y-%m-%d"),
        today_hours
    );

    let info_block = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL).title("Quick Stats").border_style(Style::default().fg(Color::DarkGray)).border_type(BorderType::Rounded));

    f.render_widget(info_block, sidebar_layout[4]);
}

fn draw_main_panel(f: &mut Frame, rect: Rect, state: &AppState) {
    match state.active_tab {
        ActiveTab::Day => draw_day_view(f, rect, state),
        ActiveTab::Week => draw_week_view(f, rect, state),
        ActiveTab::Month => draw_month_view(f, rect, state),
        ActiveTab::Dashboard => draw_dashboard_view(f, rect, state),
    }
}

// -------------------------------------------------------------
// TAB RENDERING FUNCTIONS
// -------------------------------------------------------------

fn draw_day_view(f: &mut Frame, rect: Rect, state: &AppState) {
    let day_entries: Vec<&TimeEntry> = state.time_entries.iter()
        .filter(|e| {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&e.time_interval.start) {
                let matches_date = dt.with_timezone(&chrono::Local).date_naive() == state.selected_date;
                let matches_project = match &state.selected_project_id {
                    Some(p_id) => e.project_id.as_ref() == Some(p_id),
                    None => true,
                };
                matches_date && matches_project
            } else {
                false
            }
        })
        .collect();

    let proj_filter_suffix = match &state.selected_project_id {
        Some(p_id) => format!(" [Project: {}] ", state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown")),
        None => "".to_string(),
    };
    let title = format!(" Daily Scheduler - {}{} ", state.selected_date.format("%A, %b %d, %Y"), proj_filter_suffix);
    let border_style = if state.focus == Focus::Calendar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
        .border_type(BorderType::Rounded);

    let inner_area = block.inner(rect);
    f.render_widget(block, rect);

    if day_entries.is_empty() {
        let empty_p = Paragraph::new("\n\nNo time entries logged for this day.\nPress [s] with a project selected to track time.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(empty_p, inner_area);
        return;
    }

    let list_items: Vec<ListItem> = day_entries.iter()
        .map(|entry| {
            let start = chrono::DateTime::parse_from_rfc3339(&entry.time_interval.start)
                .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_else(|_| "??:??".to_string());

            let end = if let Some(end_str) = &entry.time_interval.end {
                chrono::DateTime::parse_from_rfc3339(end_str)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
                    .unwrap_or_else(|_| "??:??".to_string())
            } else {
                "Ongoing".to_string()
            };

            let duration = entry.time_interval.duration.as_deref().unwrap_or("")
                .replace("PT", "")
                .replace("H", "h ")
                .replace("M", "m")
                .trim()
                .to_string();

            let proj_name = if let Some(p_id) = &entry.project_id {
                state.projects.iter()
                    .find(|p| &p.id == p_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Unknown Project")
            } else {
                "No Project"
            };

            let desc = entry.description.as_deref().unwrap_or("(No description)");

            let line = Line::from(vec![
                Span::styled(format!("[{}] ", proj_name), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} - {} ", start, end), Style::default().fg(Color::Green)),
                Span::styled(format!("({}) ", duration), Style::default().fg(Color::DarkGray)),
                Span::raw(desc),
            ]);

            ListItem::new(line)
        })
        .collect();

    let entry_list = List::new(list_items)
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(entry_list, inner_area);
}

fn draw_week_view(f: &mut Frame, rect: Rect, state: &AppState) {
    // 7-day side-by-side block columns
    let days_from_monday = state.selected_date.weekday().num_days_from_monday();
    let start_of_week = state.selected_date - chrono::Duration::days(days_from_monday as i64);

    let proj_filter_suffix = match &state.selected_project_id {
        Some(p_id) => format!(" [Project: {}] ", state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown")),
        None => "".to_string(),
    };
    let title = format!(
        " Weekly View - Week {} ({}){} ",
        state.selected_date.iso_week().week(),
        state.selected_date.format("%Y"),
        proj_filter_suffix
    );

    let border_style = if state.focus == Focus::Calendar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
        .border_type(BorderType::Rounded);
    let inner_area = outer_block.inner(rect);
    f.render_widget(outer_block, rect);

    // Split horizontally into 7 columns (Mon-Sun)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, 7); 7])
        .split(inner_area);

    for idx in 0..7 {
        let cur_date = start_of_week + chrono::Duration::days(idx as i64);
        
        // Sum total hours for this date
        let mut day_minutes = 0;
        let mut entries_summary = Vec::new();
        
        for entry in &state.time_entries {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&entry.time_interval.start) {
                if dt.with_timezone(&chrono::Local).date_naive() == cur_date {
                    let matches_project = match &state.selected_project_id {
                        Some(p_id) => entry.project_id.as_ref() == Some(p_id),
                        None => true,
                    };
                    if matches_project {
                        if let Some(dur) = &entry.time_interval.duration {
                            day_minutes += parse_iso_duration(dur);
                        }
                        entries_summary.push(entry.description.as_deref().unwrap_or("Untracked"));
                    }
                }
            }
        }

        let is_selected_day = cur_date == state.selected_date;
        
        let cell_border_style = if is_selected_day {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let cell_border_type = if is_selected_day && state.focus == Focus::Calendar {
            BorderType::Thick
        } else {
            BorderType::Rounded
        };

        let cell_block = Block::default()
            .borders(Borders::ALL)
            .border_style(cell_border_style)
            .border_type(cell_border_type);

        let cell_inner = cell_block.inner(columns[idx]);
        f.render_widget(cell_block, columns[idx]);

        // Layout inside column: Header date info + tasks list
        let col_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Mon \n 22
                Constraint::Length(1), // Total Hours
                Constraint::Min(3),   // Tasks summary
            ])
            .split(cell_inner);

        let date_style = if is_selected_day {
            if state.focus == Focus::Calendar {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            }
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };

        let day_display = if is_selected_day && state.focus == Focus::Calendar {
            format!("▶ {}\n{}", cur_date.format("%a"), cur_date.format("%d"))
        } else {
            format!("{}\n{}", cur_date.format("%a"), cur_date.format("%d"))
        };

        let date_widget = Paragraph::new(day_display)
            .style(date_style)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(date_widget, col_layout[0]);

        let hours = day_minutes as f64 / 60.0;
        let hours_text = if hours > 0.0 {
            format!("{:.1} hrs", hours)
        } else {
            "-".to_string()
        };

        let hours_style = if hours > 0.0 {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let hours_widget = Paragraph::new(hours_text)
            .style(hours_style)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(hours_widget, col_layout[1]);

        // Render summary items
        let summary_items: Vec<ListItem> = entries_summary.iter()
            .take(6)
            .map(|desc| ListItem::new(format!("• {}", truncate_str(desc, 12))))
            .collect();
            
        let summary_list = List::new(summary_items)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(summary_list, col_layout[2]);
    }
}

fn draw_month_view(f: &mut Frame, rect: Rect, state: &AppState) {
    let year = state.selected_date.year();
    let month = state.selected_date.month();

    let proj_filter_suffix = match &state.selected_project_id {
        Some(p_id) => format!(" [Project: {}] ", state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown")),
        None => "".to_string(),
    };
    let title = format!(" Calendar View - {} {} ", state.selected_date.format("%B %Y"), proj_filter_suffix);
    
    let outer_border_style = if state.focus == Focus::Calendar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(outer_border_style)
        .border_type(BorderType::Rounded);
    let inner_area = block.inner(rect);
    f.render_widget(block, rect);

    // Weekday headers: Mon Tue Wed Thu Fri Sat Sun
    let root_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Weekday Names Row
            Constraint::Min(5),   // Grid cells
        ])
        .split(inner_area);

    let week_days_headers = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, 7); 7])
        .split(root_layout[0]);

    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for i in 0..7 {
        let day_label = Paragraph::new(day_names[i])
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(day_label, week_days_headers[i]);
    }

    // Generate month grid
    let start_of_month = state.selected_date.with_day(1).unwrap();
    let first_weekday = start_of_month.weekday().num_days_from_monday(); // 0 = Mon, 6 = Sun
    
    // Find last day of month
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };
    let start_of_next_month = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let end_of_month = start_of_next_month - chrono::Duration::days(1);
    let total_days = end_of_month.day();

    let mut grid_days: Vec<Option<NaiveDate>> = Vec::new();
    for _ in 0..first_weekday {
        grid_days.push(None);
    }
    for d in 1..=total_days {
        grid_days.push(Some(start_of_month.with_day(d).unwrap()));
    }
    while grid_days.len() % 7 != 0 {
        grid_days.push(None);
    }

    // Grid layout splits: horizontal columns, then vertical cells
    let rows_count = grid_days.len() / 7;
    let grid_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, 7); 7])
        .split(root_layout[1]);

    for c in 0..7 {
        let grid_cells = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, rows_count as u32); rows_count])
            .split(grid_cols[c]);

        for r in 0..rows_count {
            let day_idx = r * 7 + c;
            if let Some(Some(cur_date)) = grid_days.get(day_idx) {
                // Sum total hours for this date
                let mut day_minutes = 0;
                for entry in &state.time_entries {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&entry.time_interval.start) {
                        if dt.with_timezone(&chrono::Local).date_naive() == *cur_date {
                            let matches_project = match &state.selected_project_id {
                                Some(p_id) => entry.project_id.as_ref() == Some(p_id),
                                None => true,
                            };
                            if matches_project {
                                if let Some(dur) = &entry.time_interval.duration {
                                    day_minutes += parse_iso_duration(dur);
                                }
                            }
                        }
                    }
                }

                let hours = day_minutes as f64 / 60.0;
                let is_today = cur_date == &chrono::Local::now().date_naive();
                let is_selected = cur_date == &state.selected_date;

                let cell_border_style = if is_selected {
                    Style::default().fg(Color::Yellow)
                } else if is_today {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let cell_border_type = if is_selected && state.focus == Focus::Calendar {
                    BorderType::Thick
                } else {
                    BorderType::Rounded
                };

                let cell_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(cell_border_style)
                    .border_type(cell_border_type);
                
                let cell_inner = cell_block.inner(grid_cells[r]);
                f.render_widget(cell_block, grid_cells[r]);

                let day_label = cur_date.day().to_string();
                let day_style = if is_selected {
                    if state.focus == Focus::Calendar {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Yellow)
                    }
                } else if is_today {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let day_text = if is_selected && state.focus == Focus::Calendar {
                    format!("▶ {}", day_label)
                } else {
                    day_label
                };

                let hours_label = if hours > 0.0 {
                    format!("{:.1}h", hours)
                } else {
                    "".to_string()
                };

                let hours_style = if hours > 0.0 {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let text_lines = Paragraph::new(vec![
                    Line::from(Span::styled(day_text, day_style)),
                    Line::from(Span::styled(hours_label, hours_style)),
                ])
                .alignment(ratatui::layout::Alignment::Center);

                f.render_widget(text_lines, cell_inner);
            }
        }
    }
}

fn draw_dashboard_view(f: &mut Frame, rect: Rect, state: &AppState) {
    let proj_filter_suffix = match &state.selected_project_id {
        Some(p_id) => format!(" [Project: {}] ", state.projects.iter().find(|p| &p.id == p_id).map(|p| p.name.as_str()).unwrap_or("Unknown")),
        None => "".to_string(),
    };
    let title = format!(" Dashboard Stats{} ", proj_filter_suffix);
    let border_style = if state.focus == Focus::Calendar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
        .border_type(BorderType::Rounded);
    let inner_area = block.inner(rect);
    f.render_widget(block, rect);

    // Sum statistics
    let today = chrono::Local::now().date_naive();
    
    let mut total_today_min = 0;
    let mut total_week_min = 0;
    let mut total_month_min = 0;

    let start_of_week = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let start_of_month = today.with_day(1).unwrap();

    for entry in &state.time_entries {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&entry.time_interval.start) {
            let local_date = dt.with_timezone(&chrono::Local).date_naive();
            let matches_project = match &state.selected_project_id {
                Some(p_id) => entry.project_id.as_ref() == Some(p_id),
                None => true,
            };
            if matches_project {
                if let Some(dur) = &entry.time_interval.duration {
                    let mins = parse_iso_duration(dur);
                    
                    if local_date == today {
                        total_today_min += mins;
                    }
                    if local_date >= start_of_week && local_date <= today {
                        total_week_min += mins;
                    }
                    if local_date >= start_of_month && local_date <= today {
                        total_month_min += mins;
                    }
                }
            }
        }
    }

    let stats_text = format!(
        "\n\n   Total Logged Today:         {:.1} hrs\n\n\n   Total Logged This Week:     {:.1} hrs\n\n\n   Total Logged This Month:    {:.1} hrs",
        total_today_min as f64 / 60.0,
        total_week_min as f64 / 60.0,
        total_month_min as f64 / 60.0
    );

    let stats_widget = Paragraph::new(stats_text)
        .alignment(ratatui::layout::Alignment::Left)
        .style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(stats_widget, inner_area);
}

// -------------------------------------------------------------
// HELPER METHODS
// -------------------------------------------------------------

fn parse_iso_duration(duration_str: &str) -> u64 {
    let mut hours = 0;
    let mut minutes = 0;
    
    let temp = duration_str.replace("PT", "");
    let mut current_num = String::new();
    
    for c in temp.chars() {
        if c.is_digit(10) {
            current_num.push(c);
        } else if c == 'H' {
            hours = current_num.parse::<u64>().unwrap_or(0);
            current_num.clear();
        } else if c == 'M' {
            minutes = current_num.parse::<u64>().unwrap_or(0);
            current_num.clear();
        }
    }
    
    hours * 60 + minutes
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars - 2).collect();
        format!("{}..", truncated)
    } else {
        s.to_string()
    }
}

fn get_elapsed_time_string(start_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(start_str) {
        let now = chrono::Utc::now();
        let elapsed = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
        if elapsed.num_seconds() >= 0 {
            let secs = elapsed.num_seconds() % 60;
            let mins = (elapsed.num_seconds() / 60) % 60;
            let hrs = elapsed.num_seconds() / 3600;
            return format!("{:02}:{:02}:{:02}", hrs, mins, secs);
        }
    }
    "00:00:00".to_string()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_input_modal(f: &mut Frame, state: &AppState) {
    let size = f.size();
    let popup_area = centered_rect(50, 45, size); // 50% wide, 45% high

    // Clear background
    f.render_widget(Clear, popup_area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .title(" New Time Entry (Manual) ")
        .border_style(Style::default().fg(Color::Yellow))
        .border_type(BorderType::Rounded);

    let inner_area = modal_block.inner(popup_area);
    f.render_widget(modal_block, popup_area);

    // Layout inside modal
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Description
            Constraint::Length(3), // Start Time
            Constraint::Length(3), // End Time
            Constraint::Min(1),    // Instruction footer
        ])
        .split(inner_area);

    // Field styles
    let style_desc = if state.active_modal_field == 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };
    let style_start = if state.active_modal_field == 1 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };
    let style_end = if state.active_modal_field == 2 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };

    // Description field
    let desc_p = Paragraph::new(state.modal_description.as_str())
        .block(Block::default().borders(Borders::ALL).title("Description").border_style(style_desc).border_type(BorderType::Rounded));
    f.render_widget(desc_p, chunks[0]);

    // Start Time field
    let start_p = Paragraph::new(state.modal_start_time.as_str())
        .block(Block::default().borders(Borders::ALL).title("Start Time (HH:MM)").border_style(style_start).border_type(BorderType::Rounded));
    f.render_widget(start_p, chunks[1]);

    // End Time field
    let end_p = Paragraph::new(state.modal_end_time.as_str())
        .block(Block::default().borders(Borders::ALL).title("End Time (HH:MM)").border_style(style_end).border_type(BorderType::Rounded));
    f.render_widget(end_p, chunks[2]);

    // Instructions
    let footer_p = Paragraph::new("Press [Tab] or [↑/↓] to switch fields | [Enter] Save | [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(footer_p, chunks[3]);
}
