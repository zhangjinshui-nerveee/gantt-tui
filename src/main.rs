use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{block::*, *},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, stdout};
use std::panic;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

// --- DATA STRUCTURES ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: u32,
    name: String,
    assigned_to: String,
    duration: i64,
    progress: u8,
    dependencies: Vec<u32>,
    manual_start_date: Option<NaiveDate>,
    #[serde(skip)]
    start_date: Option<NaiveDate>,
    #[serde(skip)]
    end_date: Option<NaiveDate>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ProjectData {
    project_name: String,
    project_start_date: NaiveDate,
    week_to_show: u32,
    tasks: Vec<Task>,
}

#[derive(Clone)]
struct ProjectState {
    project_name: String,
    project_start_date: NaiveDate,
    week_to_show: u32,
    tasks: Vec<Task>,
}

// --- APPLICATION STATE ---

#[derive(PartialEq, Eq, Clone, Copy)]
enum InputMode {
    Normal,
    Editing,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum TaskField {
    Name,
    AssignedTo,
    StartDate,
    Duration,
    Progress,
    Dependencies,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ProjectField {
    Name,
    StartDate,
    WeekToShow,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum FocusArea {
    Project(ProjectField),
    Tasks,
}

struct App {
    project_name: String,
    tasks: Vec<Task>,
    project_start_date: NaiveDate,
    week_to_show: u32,
    today: NaiveDate,
    table_state: TableState,
    input_mode: InputMode,
    focus_area: FocusArea,
    selected_task_field: TaskField,
    input_buffer: String,
    next_task_id: u32,
    should_quit: bool,
    status_message: String,
    gantt_area_width: u16,
    history: Vec<ProjectState>,
    redo_history: Vec<ProjectState>,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            project_name: "New Project".to_string(),
            tasks: vec![],
            project_start_date: NaiveDate::from_ymd_opt(2024, 8, 1).unwrap(),
            week_to_show: 0,
            today: Local::now().date_naive(),
            table_state: TableState::default(),
            input_mode: InputMode::Normal,
            focus_area: FocusArea::Tasks,
            selected_task_field: TaskField::Name,
            input_buffer: String::new(),
            next_task_id: 1,
            should_quit: false,
            status_message: "Welcome! Press 'q' to quit.".to_string(),
            gantt_area_width: 0,
            history: vec![],
            redo_history: vec![],
        };

        if app.load_project().is_err() {
            app.status_message = "No 'project.json' found. Starting with default tasks.".to_string();
            app.load_default_tasks();
        } else {
            app.status_message = "Project loaded successfully from 'project.json'.".to_string();
        }
        
        if !app.tasks.is_empty() {
            app.table_state.select(Some(0));
            app.focus_area = FocusArea::Tasks;
        } else {
            app.focus_area = FocusArea::Project(ProjectField::Name);
        }

        app.recalculate_schedule();
        app
    }

    fn load_default_tasks(&mut self) {
        self.add_task(Task { id: 0, name: "Requirement Gathering".into(), assigned_to: "Alice".into(), duration: 5, progress: 100, dependencies: vec![], manual_start_date: None, start_date: None, end_date: None });
        self.add_task(Task { id: 0, name: "UI/UX Design".into(), assigned_to: "Bob".into(), duration: 7, progress: 50, dependencies: vec![1], manual_start_date: None, start_date: None, end_date: None });
        self.history.clear();
    }

    fn add_task(&mut self, mut task: Task) {
        self.save_state_for_undo();
        task.id = self.next_task_id;
        self.next_task_id += 1;

        if let Some(selected_index) = self.table_state.selected() {
            self.tasks.insert(selected_index + 1, task);
        } else {
            self.tasks.push(task);
        }

        self.remap_ids_and_dependencies();
    }

    fn delete_selected_task(&mut self) {
        if let FocusArea::Tasks = self.focus_area {
            if let Some(selected_index) = self.table_state.selected() {
                if selected_index < self.tasks.len() {
                    self.save_state_for_undo();
                    self.tasks.remove(selected_index);
                    if selected_index > 0 && selected_index >= self.tasks.len() {
                        self.table_state.select(Some(self.tasks.len() - 1));
                    } else if self.tasks.is_empty() {
                        self.table_state.select(None);
                        self.focus_area = FocusArea::Project(ProjectField::WeekToShow);
                    }
                    self.remap_ids_and_dependencies();
                }
            }
        }
    }

    fn move_task_up(&mut self) {
        if let FocusArea::Tasks = self.focus_area {
            if let Some(selected_index) = self.table_state.selected() {
                if selected_index > 0 {
                    self.save_state_for_undo();
                    self.tasks.swap(selected_index, selected_index - 1);
                    self.table_state.select(Some(selected_index - 1));
                    self.remap_ids_and_dependencies();
                }
            }
        }
    }

    fn move_task_down(&mut self) {
        if let FocusArea::Tasks = self.focus_area {
            if let Some(selected_index) = self.table_state.selected() {
                if selected_index < self.tasks.len() - 1 {
                    self.save_state_for_undo();
                    self.tasks.swap(selected_index, selected_index + 1);
                    self.table_state.select(Some(selected_index + 1));
                    self.remap_ids_and_dependencies();
                }
            }
        }
    }

    fn remap_ids_and_dependencies(&mut self) {
        let id_map: HashMap<u32, u32> = self.tasks
            .iter()
            .enumerate()
            .map(|(i, task)| (task.id, (i + 1) as u32))
            .collect();

        let mut new_tasks = Vec::new();
        for (i, old_task) in self.tasks.iter().enumerate() {
            let mut new_task = old_task.clone();
            new_task.id = (i + 1) as u32;
            
            new_task.dependencies = old_task.dependencies
                .iter()
                .filter_map(|old_dep_id| id_map.get(old_dep_id).cloned())
                .collect();
                
            new_tasks.push(new_task);
        }

        self.tasks = new_tasks;
        self.recalculate_schedule();
    }

    fn recalculate_schedule(&mut self) {
        self.next_task_id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let task_map: HashMap<u32, Task> = self.tasks.iter().map(|t| (t.id, t.clone())).collect();
        let mut calculated_tasks: HashMap<u32, Task> = HashMap::new();
        let mut tasks_to_process: Vec<u32> = self.tasks.iter().map(|t| t.id).collect();
        
        let mut iterations = 0;
        while !tasks_to_process.is_empty() && iterations < 100 {
            tasks_to_process.retain(|task_id| {
                let task = task_map.get(task_id).unwrap();
                let deps_calculated = task.dependencies.iter().all(|dep_id| calculated_tasks.contains_key(dep_id) || !task_map.contains_key(dep_id));

                if deps_calculated {
                    let mut updated_task = task.clone();
                    if !task.dependencies.is_empty() {
                        let max_dep_end_date = task.dependencies.iter()
                            .filter_map(|dep_id| calculated_tasks.get(dep_id))
                            .filter_map(|dep| dep.end_date)
                            .max();
                        updated_task.start_date = Some(max_dep_end_date.map_or(self.project_start_date, |d| d + Duration::days(1)));
                    } else if let Some(manual_date) = task.manual_start_date {
                        updated_task.start_date = Some(manual_date);
                    } else {
                        updated_task.start_date = Some(self.project_start_date);
                    }
                    updated_task.end_date = updated_task.start_date.map(|d| d + Duration::days(updated_task.duration.max(1) - 1));
                    calculated_tasks.insert(*task_id, updated_task);
                    false
                } else { true }
            });
            iterations += 1;
        }

        for task in &mut self.tasks {
            if let Some(calculated) = calculated_tasks.get(&task.id) {
                task.start_date = calculated.start_date;
                task.end_date = calculated.end_date;
            } else {
                task.start_date = None;
                task.end_date = None;
            }
        }
    }

    fn save_project(&mut self) -> io::Result<()> {
        let project_data = ProjectData {
            project_name: self.project_name.clone(),
            project_start_date: self.project_start_date,
            week_to_show: self.week_to_show,
            tasks: self.tasks.clone(),
        };
        let json_data = serde_json::to_string_pretty(&project_data)?;
        fs::write("project.json", json_data)?;
        self.status_message = "Project saved successfully!".to_string();
        Ok(())
    }

    fn load_project(&mut self) -> io::Result<()> {
        let path = Path::new("project.json");
        if path.exists() {
            let json_data = fs::read_to_string(path)?;
            let project_data: ProjectData = serde_json::from_str(&json_data)?;
            self.project_name = project_data.project_name;
            self.project_start_date = project_data.project_start_date;
            self.week_to_show = project_data.week_to_show;
            self.tasks = project_data.tasks;
            self.history.clear();
            self.redo_history.clear();
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
        }
    }

    fn save_state_for_undo(&mut self) {
        self.history.push(ProjectState {
            project_name: self.project_name.clone(),
            project_start_date: self.project_start_date,
            week_to_show: self.week_to_show,
            tasks: self.tasks.clone(),
        });
        self.redo_history.clear();
    }

    fn undo(&mut self) {
        if let Some(previous_state) = self.history.pop() {
            self.redo_history.push(ProjectState {
                project_name: self.project_name.clone(),
                project_start_date: self.project_start_date,
                week_to_show: self.week_to_show,
                tasks: self.tasks.clone(),
            });
            self.project_name = previous_state.project_name;
            self.project_start_date = previous_state.project_start_date;
            self.week_to_show = previous_state.week_to_show;
            self.tasks = previous_state.tasks;
            self.recalculate_schedule();
            self.status_message = "Undo successful.".to_string();
        } else {
            self.status_message = "Nothing to undo.".to_string();
        }
    }

    fn redo(&mut self) {
        if let Some(next_state) = self.redo_history.pop() {
            self.history.push(ProjectState {
                project_name: self.project_name.clone(),
                project_start_date: self.project_start_date,
                week_to_show: self.week_to_show,
                tasks: self.tasks.clone(),
            });
            self.project_name = next_state.project_name;
            self.project_start_date = next_state.project_start_date;
            self.week_to_show = next_state.week_to_show;
            self.tasks = next_state.tasks;
            self.recalculate_schedule();
            self.status_message = "Redo successful.".to_string();
        } else {
            self.status_message = "Nothing to redo.".to_string();
        }
    }
}

// --- MAIN ---
fn main() -> io::Result<()> {
    setup_terminal()?;
    let mut app = App::new();
    run_app(&mut app)?;
    restore_terminal()?;
    Ok(())
}

fn run_app(app: &mut App) -> io::Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    while !app.should_quit {
        terminal.draw(|f| ui(f, app))?;
        handle_events(app)?;
    }
    Ok(())
}

// --- EVENT HANDLING ---
fn handle_events(app: &mut App) -> io::Result<()> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.input_mode {
                    InputMode::Normal => handle_normal_mode(app, key),
                    InputMode::Editing => handle_editing_mode(app, key),
                }
            }
        }
    }
    Ok(())
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    if key.modifiers == KeyModifiers::CONTROL {
        match key.code {
            KeyCode::Char('s') => { app.save_project().unwrap_or_else(|_| app.status_message = "Failed to save project.".into()); },
            KeyCode::Char('r') => app.redo(),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('g') => go_to_top(app),
        KeyCode::Char('G') => go_to_bottom(app),
        KeyCode::Char('K') => app.move_task_up(),
        KeyCode::Char('J') => app.move_task_down(),
        KeyCode::Char('j') | KeyCode::Down => navigate_down(app),
        KeyCode::Char('k') | KeyCode::Up => navigate_up(app),
        KeyCode::Char('h') | KeyCode::Left => select_previous_field(app),
        KeyCode::Char('l') | KeyCode::Right => select_next_field(app),
        KeyCode::Char('a') => {
            app.add_task(Task { id: 0, name: "New Task".into(), assigned_to: "Unassigned".into(), duration: 1, progress: 0, dependencies: vec![], manual_start_date: None, start_date: None, end_date: None });
            app.table_state.select(Some(app.tasks.len() - 1));
            app.focus_area = FocusArea::Tasks;
        }
        KeyCode::Char('D') => app.delete_selected_task(),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('t') => {
            if app.today < app.project_start_date {
                app.week_to_show = 0;
            } else {
                let days_from_start = (app.today - app.project_start_date).num_days();
                app.week_to_show = (days_from_start / 7) as u32;
            }
            app.status_message = format!("Jumped to the week of today's date.");
        }
        KeyCode::Enter => {
            match app.focus_area {
                FocusArea::Project(_) => {
                    app.input_mode = InputMode::Editing;
                    load_buffer_for_editing(app);
                }
                FocusArea::Tasks => {
                    if let Some(selected_index) = app.table_state.selected() {
                        let is_editable = match app.selected_task_field {
                            TaskField::StartDate => app.tasks[selected_index].dependencies.is_empty(),
                            _ => true,
                        };
                        if is_editable {
                            app.input_mode = InputMode::Editing;
                            load_buffer_for_editing(app);
                        } else {
                            app.status_message = "Cannot edit Start Date when Dependencies are set.".to_string();
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_editing_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('w') if key.modifiers == KeyModifiers::CONTROL => {
            let buffer = &mut app.input_buffer;
            let last_word_start = buffer.trim_end().rfind(' ').map_or(0, |i| i + 1);
            buffer.truncate(last_word_start);
        }
        KeyCode::Enter => {
            app.save_state_for_undo();
            save_buffer_to_task(app);
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
            app.recalculate_schedule();
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => { app.input_buffer.pop(); }
        _ => {}
    }
}

// --- STATE HELPERS ---
fn navigate_up(app: &mut App) {
    match app.focus_area {
        FocusArea::Project(ProjectField::WeekToShow) => app.focus_area = FocusArea::Project(ProjectField::StartDate),
        FocusArea::Project(ProjectField::StartDate) => app.focus_area = FocusArea::Project(ProjectField::Name),
        FocusArea::Tasks => {
            if let Some(selected) = app.table_state.selected() {
                if selected == 0 {
                    app.table_state.select(None);
                    app.focus_area = FocusArea::Project(ProjectField::WeekToShow);
                } else {
                    app.table_state.select(Some(selected - 1));
                }
            }
        }
        _ => {}
    }
}

fn navigate_down(app: &mut App) {
    match app.focus_area {
        FocusArea::Project(ProjectField::Name) => app.focus_area = FocusArea::Project(ProjectField::StartDate),
        FocusArea::Project(ProjectField::StartDate) => app.focus_area = FocusArea::Project(ProjectField::WeekToShow),
        FocusArea::Project(ProjectField::WeekToShow) => {
            if !app.tasks.is_empty() {
                app.focus_area = FocusArea::Tasks;
                app.table_state.select(Some(0));
            }
        }
        FocusArea::Tasks => {
            if let Some(selected) = app.table_state.selected() {
                if selected < app.tasks.len() - 1 {
                    app.table_state.select(Some(selected + 1));
                }
            }
        }
    }
}

fn select_next_field(app: &mut App) {
    if let FocusArea::Tasks = app.focus_area {
        app.selected_task_field = match app.selected_task_field {
            TaskField::Name => TaskField::AssignedTo,
            TaskField::AssignedTo => TaskField::StartDate,
            TaskField::StartDate => TaskField::Duration,
            TaskField::Duration => TaskField::Progress,
            TaskField::Progress => TaskField::Dependencies,
            TaskField::Dependencies => TaskField::Name,
        };
    }
}

fn select_previous_field(app: &mut App) {
    if let FocusArea::Tasks = app.focus_area {
        app.selected_task_field = match app.selected_task_field {
            TaskField::Name => TaskField::Dependencies,
            TaskField::AssignedTo => TaskField::Name,
            TaskField::StartDate => TaskField::AssignedTo,
            TaskField::Duration => TaskField::StartDate,
            TaskField::Progress => TaskField::Duration,
            TaskField::Dependencies => TaskField::Progress,
        };
    }
}

fn go_to_top(app: &mut App) {
    if !app.tasks.is_empty() {
        app.table_state.select(Some(0));
        app.focus_area = FocusArea::Tasks;
    }
}

fn go_to_bottom(app: &mut App) {
    if !app.tasks.is_empty() {
        let last_index = app.tasks.len() - 1;
        app.table_state.select(Some(last_index));
        app.focus_area = FocusArea::Tasks;
    }
}

fn load_buffer_for_editing(app: &mut App) {
    match app.focus_area {
        FocusArea::Project(ProjectField::Name) => app.input_buffer = app.project_name.clone(),
        FocusArea::Project(ProjectField::StartDate) => app.input_buffer = app.project_start_date.format("%m/%d/%Y").to_string(),
        FocusArea::Project(ProjectField::WeekToShow) => app.input_buffer = app.week_to_show.to_string(),
        FocusArea::Tasks => {
            if let Some(index) = app.table_state.selected() {
                let task = &app.tasks[index];
                app.input_buffer = match app.selected_task_field {
                    TaskField::Name => task.name.clone(),
                    TaskField::AssignedTo => task.assigned_to.clone(),
                    TaskField::Duration => task.duration.to_string(),
                    TaskField::Progress => task.progress.to_string(),
                    TaskField::Dependencies => task.dependencies.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", "),
                    TaskField::StartDate => task.manual_start_date.map_or("".to_string(), |d| d.format("%m/%d/%Y").to_string()),
                };
            }
        }
    }
}

fn save_buffer_to_task(app: &mut App) {
    match app.focus_area {
        FocusArea::Project(ProjectField::Name) => app.project_name = app.input_buffer.clone(),
        FocusArea::Project(ProjectField::StartDate) => {
            if let Ok(date) = NaiveDate::parse_from_str(&app.input_buffer, "%m/%d/%Y") {
                app.project_start_date = date;
            } else {
                app.status_message = "Invalid date format. Please use mm/dd/yyyy.".to_string();
            }
        }
        FocusArea::Project(ProjectField::WeekToShow) => {
            if let Ok(week) = app.input_buffer.parse() {
                app.week_to_show = week;
            } else {
                app.status_message = "Invalid number for week.".to_string();
            }
        }
        FocusArea::Tasks => {
            if let Some(index) = app.table_state.selected() {
                let task = &mut app.tasks[index];
                match app.selected_task_field {
                    TaskField::Name => task.name = app.input_buffer.clone(),
                    TaskField::AssignedTo => task.assigned_to = app.input_buffer.clone(),
                    TaskField::Duration => task.duration = app.input_buffer.parse().unwrap_or(task.duration),
                    TaskField::Progress => task.progress = app.input_buffer.parse().unwrap_or(task.progress).min(100),
                    TaskField::Dependencies => {
                        task.dependencies = app.input_buffer.split(',')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect();
                        if !task.dependencies.is_empty() {
                            task.manual_start_date = None;
                        }
                    }
                    TaskField::StartDate => {
                        if app.input_buffer.is_empty() {
                            task.manual_start_date = None;
                        } else if let Ok(date) = NaiveDate::parse_from_str(&app.input_buffer, "%m/%d/%Y") {
                            task.manual_start_date = Some(date);
                            task.dependencies.clear();
                            app.status_message = "Dependencies cleared for task with manual start date.".to_string();
                        } else {
                            app.status_message = "Invalid date format. Please use mm/dd/yyyy.".to_string();
                        }
                    }
                }
            }
        }
    }
}

// --- UI RENDERING ---
fn calculate_column_widths(app: &App) -> [u16; 7] {
    const PADDING: u16 = 2;
    let id_col_width = app.tasks.iter()
        .map(|t| UnicodeWidthStr::width(t.id.to_string().as_str()))
        .max().unwrap_or(0).max(UnicodeWidthStr::width("ID")) as u16 + PADDING;

    let name_col_width = app.tasks.iter()
        .map(|t| UnicodeWidthStr::width(t.name.as_str()))
        .max().unwrap_or(0).max(UnicodeWidthStr::width("Name")) as u16 + 12 + PADDING;

    let assigned_col_width = app.tasks.iter()
        .map(|t| UnicodeWidthStr::width(t.assigned_to.as_str()))
        .max().unwrap_or(0).max(UnicodeWidthStr::width("Assigned")) as u16 + PADDING;

    let start_col_width = UnicodeWidthStr::width("mm/dd/yyyy") as u16 + PADDING;
    let dur_col_width = UnicodeWidthStr::width("Dur").max(4) as u16 + PADDING;
    let prog_col_width = UnicodeWidthStr::width("Prog%").max(4) as u16 + PADDING;
    
    let deps_col_width = app.tasks.iter()
        .map(|t| {
            if t.dependencies.is_empty() { 0 }
            else {
                t.dependencies.iter().map(|d| UnicodeWidthStr::width(d.to_string().as_str())).sum::<usize>() 
                + (t.dependencies.len() - 1) * 2
            }
        })
        .max().unwrap_or(0).max(UnicodeWidthStr::width("Deps")) as u16 + PADDING;

    [id_col_width, name_col_width, assigned_col_width, start_col_width, dur_col_width, prog_col_width, deps_col_width]
}

// --- UI RENDERING ---
fn ui(frame: &mut Frame, app: &mut App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(frame.area());

    let total_width = frame.area().width;
    let min_right_width = (total_width as f32 * 0.3) as u16;

    let column_widths = calculate_column_widths(app);
    let ideal_left_width: u16 = column_widths.iter().sum();
    
    let mut left_width = ideal_left_width;
    if total_width.saturating_sub(left_width) < min_right_width {
        left_width = total_width.saturating_sub(min_right_width);
    }

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_width), Constraint::Min(0)])
        .split(main_layout[0]);

    let table_area = main_chunks[0];
    render_task_table(frame, table_area, app, &column_widths);
    render_gantt_chart(frame, main_chunks[1], app);
    render_footer(frame, main_layout[1], app);

    if let InputMode::Editing = app.input_mode {
        match app.focus_area {
            FocusArea::Project(field) => {
                let y_offset = match field {
                    ProjectField::Name => 1,
                    ProjectField::StartDate => 2,
                    ProjectField::WeekToShow => 3,
                };
                let x_offset = match field {
                    ProjectField::Name => "Project: ".len(),
                    ProjectField::StartDate => "Start Date: ".len(),
                    ProjectField::WeekToShow => "Week to Show: ".len(),
                };
                frame.set_cursor(
                    table_area.x + 1 + (x_offset + app.input_buffer.len()) as u16,
                    table_area.y + y_offset,
                );
            }
            FocusArea::Tasks => {
                if let Some(selected_row_index) = app.table_state.selected() {
                    let block = Block::default().borders(Borders::ALL);
                    let inner_area = block.inner(table_area);
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(0),
                        ])
                        .split(inner_area);
                    let tasks_area = layout[4];

                    let col_constraints: Vec<Constraint> = column_widths.iter().map(|w| Constraint::Length(*w)).collect();
                    let col_layout = Layout::default().direction(Direction::Horizontal).constraints(col_constraints).split(tasks_area);

                    let selected_col_index = app.selected_task_field as usize + 1;
                    let selected_col_rect = col_layout[selected_col_index];

                    let mut cursor_x = selected_col_rect.x + "> ".len() as u16 + app.input_buffer.len() as u16;
                    match app.selected_task_field {
                        TaskField::Name => cursor_x += 1,
                        TaskField::AssignedTo => cursor_x -= 4,
                        TaskField::StartDate => cursor_x -= 3,
                        TaskField::Duration => cursor_x -= 2,
                        TaskField::Progress => cursor_x -= 1,
                        _ => {}
                    }
                    let cursor_y = tasks_area.y + selected_row_index as u16;
                    frame.set_cursor(cursor_x, cursor_y);
                }
            }
        }
    }
}

fn render_task_table(frame: &mut Frame, area: Rect, app: &App, column_widths: &[u16; 7]) {
    let block = Block::default().borders(Borders::ALL).title("Project Details & Tasks");
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Project Name
            Constraint::Length(1), // Project Start Date
            Constraint::Length(1), // Week to Show
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Tasks
        ])
        .split(inner_area);

    let name_style = if app.focus_area == FocusArea::Project(ProjectField::Name) { Style::default().bg(Color::Blue) } else { Style::default() };
    let start_date_style = if app.focus_area == FocusArea::Project(ProjectField::StartDate) { Style::default().bg(Color::Blue) } else { Style::default() };
    let week_style = if app.focus_area == FocusArea::Project(ProjectField::WeekToShow) { Style::default().bg(Color::Blue) } else { Style::default() };
    
    let name_text = if app.focus_area == FocusArea::Project(ProjectField::Name) && app.input_mode == InputMode::Editing { &app.input_buffer } else { &app.project_name };
    let start_date_text = if app.focus_area == FocusArea::Project(ProjectField::StartDate) && app.input_mode == InputMode::Editing { app.input_buffer.clone() } else { app.project_start_date.format("%m/%d/%Y").to_string() };
    let week_text = if app.focus_area == FocusArea::Project(ProjectField::WeekToShow) && app.input_mode == InputMode::Editing { app.input_buffer.clone() } else { app.week_to_show.to_string() };

    frame.render_widget(Paragraph::new(format!("Project: {}", name_text)).style(name_style), layout[0]);
    frame.render_widget(Paragraph::new(format!("Start Date: {}", start_date_text)).style(start_date_style), layout[1]);
    frame.render_widget(Paragraph::new(format!("Week to Show: {}", week_text)).style(week_style), layout[2]);

    let header_area = layout[3];
    let tasks_area = layout[4];

    let constraints = [
        Constraint::Length(column_widths[0]),
        Constraint::Length(column_widths[1]),
        Constraint::Length(column_widths[2]),
        Constraint::Length(column_widths[3]),
        Constraint::Length(column_widths[4]),
        Constraint::Length(column_widths[5]),
        Constraint::Length(column_widths[6]),
    ];

    let header_cells = ["ID", "Name", "Assigned", "Start", "Dur", "Prog%", "Deps"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).style(Style::default().bg(Color::LightBlue)).height(1);
    let header_table = Table::new(vec![header_row], constraints.clone());
    frame.render_widget(header_table, header_area);

    let rows = app.tasks.iter().enumerate().map(|(i, task)| {
        let is_selected_row = app.table_state.selected() == Some(i);
        let is_today_task = task.start_date.map_or(false, |start| {
            task.end_date.map_or(false, |end| app.today >= start && app.today <= end)
        });

        let deps_str = task.dependencies.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ");
        
        let id_cell = Cell::from(format!(" {}", task.id));

        let cells_data = vec![
            (TaskField::Name, task.name.clone()),
            (TaskField::AssignedTo, task.assigned_to.clone()),
            (TaskField::StartDate, task.start_date.map_or_else(|| "-".to_string(), |d| d.format("%m/%d/%Y").to_string())),
            (TaskField::Duration, task.duration.to_string()),
            (TaskField::Progress, task.progress.to_string()),
            (TaskField::Dependencies, deps_str),
        ];

        let mut other_cells: Vec<Cell> = cells_data.iter().map(|(field, data)| {
            let is_active_cell = is_selected_row && app.selected_task_field == *field;
            let style = if is_active_cell {
                match app.input_mode {
                    InputMode::Editing => Style::default().fg(Color::Black).bg(Color::Yellow),
                    InputMode::Normal => Style::default().bg(Color::Blue),
                }
            } else { Style::default() };

            let content_text = if is_active_cell {
                let text = if let InputMode::Editing = app.input_mode { &app.input_buffer } else { data };
                format!("> {}", text)
            } else {
                format!(" {}", data)
            };
            
            Cell::from(content_text).style(style)
        }).collect();
        
        let mut all_cells = vec![id_cell];
        all_cells.append(&mut other_cells);

        let row_style = if is_today_task {
            Style::default().fg(Color::Rgb(173, 216, 230))
        } else {
            Style::default()
        };

        Row::new(all_cells).style(row_style)
    });

    let table = Table::new(rows, constraints)
        .row_highlight_style(Style::default().bg(Color::Rgb(50, 50, 50)).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(table, tasks_area, &mut app.table_state.clone());
}

fn render_gantt_chart(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().title("Gantt Chart Timeline").borders(Borders::ALL);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let chart_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(inner_area);
    let header_area = chart_layout[0];
    let content_area = chart_layout[1];
    
    app.gantt_area_width = content_area.width;
    let min_date = app.project_start_date + Duration::weeks(app.week_to_show as i64);
    
    const DAY_WIDTH: u16 = 4;
    let date_range_days = (app.gantt_area_width / DAY_WIDTH) as i64;

    let mut month_spans = vec![];
    let mut day_spans = vec![];
    let mut weekday_spans = vec![];
    let mut last_month = 0;

    for day in 0..=date_range_days {
        let current_date = min_date + Duration::days(day);
        let is_today = current_date == app.today;
        let day_style = if is_today { Style::default().fg(Color::Black).bg(Color::Cyan) } else { Style::default() };

        let weekday_char = match current_date.weekday() {
            Weekday::Mon => "M",
            Weekday::Tue => "T",
            Weekday::Wed => "W",
            Weekday::Thu => "T",
            Weekday::Fri => "F",
            Weekday::Sat => "S",
            Weekday::Sun => "S",
        };

        day_spans.push(Span::styled(format!(" {:<2} ", current_date.day()), day_style));
        weekday_spans.push(Span::styled(format!(" {}  ", weekday_char), day_style));

        if current_date.month() != last_month {
            last_month = current_date.month();
            month_spans.push(Span::styled(format!("|{:<width$}", current_date.format("%b"), width = DAY_WIDTH as usize - 1), Style::default()));
        } else {
            month_spans.push(Span::raw(" ".repeat(DAY_WIDTH as usize)));
        }
    }
    
    let header_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).split(header_area);
    frame.render_widget(Paragraph::new(Line::from(month_spans)).scroll((0, 0)), header_layout[0]);
    frame.render_widget(Paragraph::new(Line::from(day_spans)).scroll((0, 0)), header_layout[1]);
    frame.render_widget(Paragraph::new(Line::from(weekday_spans)).scroll((0, 0)), header_layout[2]);

    let mut lines = vec![Line::from(""); 1]; // 1 for header alignment

    for (i, task) in app.tasks.iter().enumerate() {
        let row_style = if app.focus_area == FocusArea::Tasks && app.table_state.selected() == Some(i) { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
        let mut bar_spans = vec![];
        if let (Some(start), Some(end)) = (task.start_date, task.end_date) {
            let progress_duration = (task.duration as f32 * (task.progress as f32 / 100.0)).round() as i64;
            let progress_end = if progress_duration > 0 {
                start + Duration::days(progress_duration - 1)
            } else {
                start - Duration::days(1)
            };

            for day in 0..=date_range_days {
                let current_date = min_date + Duration::days(day);
                let is_today = current_date == app.today;
                let is_task_day = current_date >= start && current_date <= end;
                
                let content = if is_task_day {
                    let is_progress_day = current_date <= progress_end;
                    if is_today {
                        if is_progress_day { "|░░|" } else { "|██|" }
                    } else {
                        if is_progress_day { "░░░░" } else { "████" }
                    }
                } else {
                    if is_today { "|  |" } else { "    " }
                };

                let style = if is_today { row_style.fg(Color::Cyan) } else { row_style };
                bar_spans.push(Span::styled(content, style));
            }
        }
        lines.push(Line::from(bar_spans).style(row_style));
    }

    frame.render_widget(Paragraph::new(lines), content_area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help_text = match app.input_mode {
        InputMode::Normal => "Nav (j/k/h/l) | (a)dd | (D)elete | (t)oday | (u)ndo | (Ctrl-r)edo | (Enter) edit | (Ctrl-s) save | (q)uit",
        InputMode::Editing => "Editing... (Enter) save | (Esc) cancel | (Ctrl-w) del word",
    };
    
    let layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);
    frame.render_widget(Paragraph::new(app.status_message.clone()).alignment(Alignment::Left), layout[0]);
    frame.render_widget(Paragraph::new(help_text).alignment(Alignment::Right), layout[1]);
}

// --- TERMINAL SETUP & RESTORATION ---
fn setup_terminal() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
    Ok(())
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
