use std::{error::Error, io, path::PathBuf, time::Duration};

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use unicode_width::UnicodeWidthStr;

const ROW_LIMIT: usize = 1_000;

/// Read-only terminal UI for browsing SQLite databases.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Path to the SQLite database file to open.
    database: PathBuf,
}

#[derive(Default)]
struct TableData {
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct App {
    database_path: String,
    tables: Vec<String>,
    table_index: usize,
    tab_offset: usize,
    data: TableData,
    row_state: TableState,
    filter: String,
    filter_draft: String,
    editing_filter: bool,
    status: String,
}

impl App {
    fn new(database_path: String, connection: &Connection) -> rusqlite::Result<Self> {
        let mut app = Self {
            database_path,
            tables: Vec::new(),
            table_index: 0,
            tab_offset: 0,
            data: TableData::default(),
            row_state: TableState::default(),
            filter: String::new(),
            filter_draft: String::new(),
            editing_filter: false,
            status: String::new(),
        };
        app.reload(connection)?;
        Ok(app)
    }

    fn reload(&mut self, connection: &Connection) -> rusqlite::Result<()> {
        let previous = self.tables.get(self.table_index).cloned();
        self.tables = table_names(connection)?;
        self.table_index = previous
            .and_then(|name| self.tables.iter().position(|table| table == &name))
            .unwrap_or(0);
        self.tab_offset = self.tab_offset.min(self.table_index);
        self.load_current_table(connection)?;
        self.status = format!("reloaded {}", self.database_path);
        Ok(())
    }

    fn load_current_table(&mut self, connection: &Connection) -> rusqlite::Result<()> {
        self.data = match self.tables.get(self.table_index) {
            Some(name) => load_table(connection, name, &self.filter)?,
            None => TableData::default(),
        };
        self.row_state
            .select((!self.data.rows.is_empty()).then_some(0));
        Ok(())
    }

    fn select_table(&mut self, connection: &Connection, delta: isize) -> rusqlite::Result<()> {
        if self.tables.is_empty() {
            return Ok(());
        }
        self.table_index = self
            .table_index
            .saturating_add_signed(delta)
            .min(self.tables.len() - 1);
        self.tab_offset = self.tab_offset.min(self.table_index);
        self.load_current_table(connection)
    }

    fn select_row(&mut self, delta: isize) {
        if self.data.rows.is_empty() {
            return;
        }
        let current = self.row_state.selected().unwrap_or(0);
        self.row_state.select(Some(
            current
                .saturating_add_signed(delta)
                .min(self.data.rows.len() - 1),
        ));
    }

    fn begin_filter_edit(&mut self) {
        self.filter_draft.clone_from(&self.filter);
        self.editing_filter = true;
    }

    fn apply_filter(&mut self, connection: &Connection) -> rusqlite::Result<()> {
        let previous = self.filter.clone();
        self.filter = self.filter_draft.trim().to_owned();
        if let Err(error) = self.load_current_table(connection) {
            self.filter = previous;
            return Err(error);
        }
        self.editing_filter = false;
        self.status = if self.filter.is_empty() {
            "filter cleared".to_owned()
        } else {
            format!("filter applied: {}", self.filter)
        };
        Ok(())
    }

    fn clear_filter(&mut self, connection: &Connection) -> rusqlite::Result<()> {
        if self.filter.is_empty() {
            self.status = "no filter to clear".to_owned();
            return Ok(());
        }
        self.filter.clear();
        self.filter_draft.clear();
        self.load_current_table(connection)?;
        self.status = "filter cleared".to_owned();
        Ok(())
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn table_names(connection: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    statement.query_map([], |row| row.get(0))?.collect()
}

fn load_table(connection: &Connection, name: &str, filter: &str) -> rusqlite::Result<TableData> {
    let where_clause = (!filter.is_empty()).then(|| format!(" WHERE {filter}"));
    let sql = format!(
        "SELECT * FROM {}{} LIMIT {ROW_LIMIT}",
        quote_identifier(name),
        where_clause.unwrap_or_default()
    );
    let mut statement = connection.prepare(&sql)?;
    let columns = statement
        .column_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    let count = statement.column_count();
    let rows = statement
        .query_map([], |row| {
            (0..count)
                .map(|index| display_value(row.get_ref(index)?))
                .collect::<rusqlite::Result<Vec<_>>>()
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(TableData {
        name: name.to_owned(),
        columns,
        rows,
    })
}

fn display_value(value: ValueRef<'_>) -> rusqlite::Result<String> {
    Ok(match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
        ValueRef::Blob(value) => format!("<BLOB {} bytes>", value.len()),
    })
}

fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(frame.area());
    draw_tabs(frame, app, areas[0]);
    draw_rows(frame, app, areas[1]);
    draw_detail(frame, app, areas[2]);
    let help = if app.editing_filter {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.filter_draft),
            Span::styled("  Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" apply  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ])
    } else {
        Line::from(vec![
            Span::styled("←/→", Style::default().fg(Color::Cyan)),
            Span::raw(" table  "),
            Span::styled("↑/↓ PgUp/PgDn", Style::default().fg(Color::Cyan)),
            Span::raw(" row  "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(" reload  "),
            Span::styled("f", Style::default().fg(Color::Cyan)),
            Span::raw(" filter  "),
            Span::styled("c", Style::default().fg(Color::Cyan)),
            Span::raw(" clear  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit  "),
            Span::styled(&app.status, Style::default().fg(Color::DarkGray)),
        ])
    };
    frame.render_widget(Paragraph::new(help), areas[3]);
}

fn tab_width(name: &str) -> usize {
    UnicodeWidthStr::width(name) + 2
}

fn draw_tabs(frame: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let available = usize::from(area.width.saturating_sub(4));
    while app.tab_offset < app.table_index
        && app.tables[app.tab_offset..=app.table_index]
            .iter()
            .map(|name| tab_width(name))
            .sum::<usize>()
            > available
    {
        app.tab_offset += 1;
    }

    let tabs = if app.tables.is_empty() {
        Line::from("No user tables")
    } else {
        let has_previous = app.tab_offset > 0;
        let prefix_width = usize::from(has_previous) * 2;
        let mut used = prefix_width;
        let mut spans = if has_previous {
            vec![Span::styled("‹ ", Style::default().fg(Color::DarkGray))]
        } else {
            Vec::new()
        };
        let mut has_next = false;
        for (index, name) in app.tables.iter().enumerate().skip(app.tab_offset) {
            let width = tab_width(name);
            let has_more = index + 1 < app.tables.len();
            let reserve_for_more = usize::from(has_more) * 2;
            if used + width + reserve_for_more > available && index != app.table_index {
                has_next = true;
                break;
            }
            used += width;
            let style = if index == app.table_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.extend([Span::raw(" "), Span::styled(name, style), Span::raw(" ")]);
        }
        if has_next {
            spans.push(Span::styled(" ›", Style::default().fg(Color::DarkGray)));
        }
        Line::from(spans)
    };
    frame.render_widget(
        Paragraph::new(tabs).block(Block::default().borders(Borders::ALL).title(format!(
            " Tables — filter: {} ",
            if app.filter.is_empty() {
                "none"
            } else {
                &app.filter
            }
        ))),
        area,
    );
}

fn draw_rows(frame: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let widths = column_widths(&app.data, area.width.saturating_sub(3));
    let header = Row::new(
        app.data
            .columns
            .iter()
            .map(|value| Cell::from(value.as_str())),
    )
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let rows = app
        .data
        .rows
        .iter()
        .map(|row| Row::new(row.iter().map(|value| Cell::from(value.as_str()))));
    let title = format!(
        " {} ({} rows{}) ",
        app.data.name,
        app.data.rows.len(),
        if app.data.rows.len() == ROW_LIMIT {
            ", limited"
        } else {
            ""
        }
    );
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_stateful_widget(table, area, &mut app.row_state);
}

fn column_widths(data: &TableData, available: u16) -> Vec<Constraint> {
    if data.columns.is_empty() {
        return Vec::new();
    }
    let width = (available / data.columns.len() as u16).max(8);
    data.columns
        .iter()
        .map(|_| Constraint::Length(width))
        .collect()
}

fn draw_detail(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let text = app
        .row_state
        .selected()
        .and_then(|index| app.data.rows.get(index))
        .map(|row| {
            app.data
                .columns
                .iter()
                .zip(row)
                .map(|(column, value)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{column}: "),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(value),
                    ])
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![Line::from("No row selected")]);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Selected row "),
        ),
        area,
    );
}

fn run(database_path: String) -> Result<(), Box<dyn Error>> {
    let connection = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut app = App::new(database_path, &connection)?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            terminal.draw(|frame| draw(frame, &mut app))?;
            if !event::poll(Duration::from_millis(250))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.editing_filter {
                match key.code {
                    KeyCode::Esc => app.editing_filter = false,
                    KeyCode::Enter => match app.apply_filter(&connection) {
                        Ok(()) => {}
                        Err(error) => app.status = format!("invalid filter: {error}"),
                    },
                    KeyCode::Backspace => {
                        app.filter_draft.pop();
                    }
                    KeyCode::Char(character) => app.filter_draft.push(character),
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Left | KeyCode::Char('h') => app.select_table(&connection, -1)?,
                KeyCode::Right | KeyCode::Char('l') => app.select_table(&connection, 1)?,
                KeyCode::Up | KeyCode::Char('k') => app.select_row(-1),
                KeyCode::Down | KeyCode::Char('j') => app.select_row(1),
                KeyCode::PageUp => app.select_row(-10),
                KeyCode::PageDown => app.select_row(10),
                KeyCode::Home => app
                    .row_state
                    .select((!app.data.rows.is_empty()).then_some(0)),
                KeyCode::End => app.row_state.select(app.data.rows.len().checked_sub(1)),
                KeyCode::Char('r') => match app.reload(&connection) {
                    Ok(()) => {}
                    Err(error) => app.status = format!("reload failed: {error}"),
                },
                KeyCode::Char('f') => app.begin_filter_edit(),
                KeyCode::Char('c') => match app.clear_filter(&connection) {
                    Ok(()) => {}
                    Err(error) => app.status = format!("could not clear filter: {error}"),
                },
                _ => {}
            }
        }
        Ok(())
    })();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args.database.display().to_string()) {
        eprintln!("sqlite-reader: {error}");
        std::process::exit(1);
    }
}
