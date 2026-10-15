use crate::game::{Game, GameMode, Owner, PieceType, Loc};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};
type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

static mut TUI_TERMINAL: Option<TuiTerminal> = None;

fn get_terminal() -> &'static mut TuiTerminal {
    unsafe {
        if TUI_TERMINAL.is_none() {
            let backend = CrosstermBackend::new(std::io::stdout());
            TUI_TERMINAL = Some(Terminal::new(backend).unwrap());
        }
        TUI_TERMINAL.as_mut().unwrap()
    }
}

pub fn refresh_display_with_input(game: &Game, prompt: &str) {
    refresh_display_with_input_blinking(game, prompt, None);
}

pub fn refresh_display_with_input_blinking(game: &Game, prompt: &str, active_blink: Option<(Loc, bool)>) {
    let terminal = get_terminal();

    terminal.draw(|f| {
        let size = f.size();
        
        // Clear screen buffer to prevent background text bleed
        f.render_widget(Clear, size);

        // Top-level vertical split: [Upper Section (Map + Sidebar), Messages, Prompt]
        let root_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(12),
                Constraint::Length(7),
                Constraint::Length(2),
            ])
            .split(size);

        let upper_area = root_chunks[0];
        let msg_area = root_chunks[1];
        let prompt_area = root_chunks[2];

        // Horizontal split for upper section: [Map View, Right-hand Sidebar / Legend]
        let upper_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(40),
                Constraint::Length(30),
            ])
            .split(upper_area);

        let map_area = upper_chunks[0];
        let sidebar_area = upper_chunks[1];

        // Center camera on edit cursor, or active unit if blinking, otherwise player capital
        let center_loc = if game.mode == GameMode::Edit || matches!(game.mode, GameMode::PickDest(_)) {
            game.edit_cursor
        } else if let Some((loc, _)) = active_blink {
            loc
        } else {
            game.cities.iter()
                .find(|c| c.owner == Owner::Player)
                .map(|c| c.loc)
                .unwrap_or(Loc { r: game.rows / 2, c: game.cols / 2 })
        };
        let view_height = map_area.height.saturating_sub(2) as usize;
        let view_width = map_area.width.saturating_sub(2) as usize;

        let half_h = view_height / 2;
        let half_w = view_width / 2;

        let start_r = center_loc.r.saturating_sub(half_h).min(game.rows.saturating_sub(view_height));
        let end_r = (start_r + view_height).min(game.rows);

        let start_c = center_loc.c.saturating_sub(half_w).min(game.cols.saturating_sub(view_width));
        let end_c = (start_c + view_width).min(game.cols);

        let mut lines = Vec::new();
        for r in start_r..end_r {
            let mut spans = Vec::new();
            for c in start_c..end_c {
                let loc = Loc { r, c };
                let is_explored = game.player_map[r][c] == 1;

                if !is_explored {
                    spans.push(Span::raw(" "));
                    continue;
                }

                let mut rendered_span = None;
                if (game.mode == GameMode::Edit || matches!(game.mode, GameMode::PickDest(_))) && game.edit_cursor == loc {
                    rendered_span = Some(Span::styled("█", Style::default().fg(Color::White)));
                }

                if let Some(span) = rendered_span {
                    spans.push(span);
                    continue;
                }

                let is_blink_off = active_blink.map_or(false, |(b_loc, show)| b_loc == loc && !show);

                // Check pieces
                let piece_opt = game.pieces.iter().enumerate().find(|(i, p)| p.loc == loc && game.is_visible_to(*i, Owner::Player)).map(|(_, p)| p);
                if let Some(p) = piece_opt {
                    if !is_blink_off {
                        let ch = match p.ptype {
                            PieceType::Army => if p.owner == Owner::Player { 'A' } else { 'a' },
                            PieceType::Fighter => if p.owner == Owner::Player { 'F' } else { 'f' },
                            PieceType::PatrolBoat => if p.owner == Owner::Player { 'P' } else { 'p' },
                            PieceType::Destroyer => if p.owner == Owner::Player { 'D' } else { 'd' },
                            PieceType::Submarine => if p.owner == Owner::Player { 'S' } else { 's' },
                            PieceType::Carrier => if p.owner == Owner::Player { 'C' } else { 'c' },
                            PieceType::Battleship => if p.owner == Owner::Player { 'B' } else { 'b' },
                            PieceType::Transport => if p.owner == Owner::Player { 'T' } else { 't' },
                            PieceType::Satellite => if p.owner == Owner::Player { 'Z' } else { 'z' },
                        };

                        let color = if p.owner == Owner::Player { Color::Cyan } else { Color::LightRed };
                        spans.push(Span::styled(ch.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)));
                        continue;
                    }
                }

                // Check cities
                let city_opt = game.cities.iter().find(|ct| ct.loc == loc);
                if let Some(city) = city_opt {
                    if !(is_blink_off && piece_opt.is_none()) {
                        let ch = match city.owner {
                            Owner::Player => 'O',
                            Owner::Comp => 'X',
                            Owner::Neutral => '*',
                        };

                        let color = match city.owner {
                            Owner::Player => Color::Green,
                            Owner::Comp => Color::Red,
                            Owner::Neutral => Color::Yellow,
                        };
                        spans.push(Span::styled(ch.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)));
                        continue;
                    }
                }

                // Terrain
                let terrain_ch = game.map[r][c] as char;
                if terrain_ch == '#' {
                    spans.push(Span::styled("#", Style::default().fg(Color::Yellow)));
                } else {
                    spans.push(Span::styled(".", Style::default().fg(Color::Blue)));
                }
            }
            lines.push(Line::from(spans));
        }

        let map_title = format!(" VMS Empire | Turn: {} | Mode: {:?} ", game.turn, game.mode);
        let map_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(map_title));
        f.render_widget(map_widget, map_area);

        // Right-hand Sidebar / Legend Widget
        let player_armies = game.pieces.iter().filter(|p| p.owner == Owner::Player && p.ptype == PieceType::Army).count();
        let player_cities = game.cities.iter().filter(|c| c.owner == Owner::Player).count();
        let enemy_cities = game.cities.iter().filter(|c| c.owner == Owner::Comp).count();
let mut sidebar_lines = vec![
            Line::from(Span::styled("--- STATUS ---", Style::default().fg(Color::Yellow))),
            Line::from(format!(" Armies: {}", player_armies)),
            Line::from(format!(" My Cities: {}", player_cities)),
            Line::from(format!(" Enemy Cities: {}", enemy_cities)),
            Line::from(""),
        ];

        // NEW: Dynamic Cursor Info Panel for Edit Mode
        if game.mode == GameMode::Edit {
            sidebar_lines.push(Line::from(Span::styled("--- CURSOR INFO ---", Style::default().fg(Color::Magenta))));
            let loc = game.edit_cursor;
            
            // Only reveal details if the tile is explored (no cheating!)
            if game.player_map[loc.r][loc.c] == 1 {
                let terrain = if game.map[loc.r][loc.c] == b'#' { "Land" } else { "Sea" };
                sidebar_lines.push(Line::from(format!(" Terrain: {}", terrain)));

                if let Some(city) = game.cities.iter().find(|c| c.loc == loc) {
                    let owner_str = match city.owner { Owner::Player => "Player", Owner::Comp => "Enemy", Owner::Neutral => "Neutral" };
                    sidebar_lines.push(Line::from(format!(" City: {}", owner_str)));
                    if city.owner == Owner::Player {
                        sidebar_lines.push(Line::from(format!(" Prod: {:?} ({}/{})", city.prod, city.work, city.prod.build_cost())));
                    }
                }
                
                if let Some(piece) = game.pieces.iter().enumerate().find(|(i, p)| p.loc == loc && p.hits > 0 && game.is_visible_to(*i, Owner::Player)).map(|(_, p)| p) {
                    let owner_str = match piece.owner { Owner::Player => "Player", Owner::Comp => "Enemy", Owner::Neutral => "Neutral" };
                    sidebar_lines.push(Line::from(format!(" Unit: {} {:?}", owner_str, piece.ptype)));
                    if piece.owner == Owner::Player {
                        sidebar_lines.push(Line::from(format!(" Hits: {}/{}", piece.hits, piece.ptype.max_hits())));
                        if piece.ptype == PieceType::Fighter {
                            sidebar_lines.push(Line::from(format!(" Fuel: {}/32", piece.fuel)));
                        } else if piece.ptype == PieceType::Satellite {
                            sidebar_lines.push(Line::from(format!(" Orbit: {}/20", piece.fuel)));
                        }
                        sidebar_lines.push(Line::from(format!(" Moves: {}/{}", piece.moves_left, piece.ptype.max_moves())));
                        sidebar_lines.push(Line::from(format!(" Order: {:?}", piece.activity)));
                        if !piece.cargo.is_empty() {
                            sidebar_lines.push(Line::from(format!(" Cargo: {} units", piece.cargo.len())));
                        }
                    }
                }
            } else {
                sidebar_lines.push(Line::from(" Terrain: Fog of War"));
            }
            sidebar_lines.push(Line::from(""));
        }

        sidebar_lines.extend(vec![
            Line::from(Span::styled("--- LEGEND ---", Style::default().fg(Color::Yellow))),
            Line::from(Span::styled(" O : Player City", Style::default().fg(Color::Green))),
            Line::from(Span::styled(" X : Enemy City", Style::default().fg(Color::Red))),
            Line::from(Span::styled(" * : Neutral City", Style::default().fg(Color::Yellow))),
            Line::from(Span::styled(" A : Player Army", Style::default().fg(Color::Cyan))),
            Line::from(Span::styled(" a : Enemy Army", Style::default().fg(Color::LightRed))),
            Line::from(Span::styled(" # : Land / . : Sea", Style::default().fg(Color::DarkGray))),
        ]);
        let sidebar_widget = Paragraph::new(sidebar_lines)
            .block(Block::default().borders(Borders::ALL).title(" Info "));
        f.render_widget(sidebar_widget, sidebar_area);

        // Message Log Widget
        let total_msgs = game.messages.len();
        let display_count = 5; // Your UI box holds exactly 5 lines inside its borders
        
        // Calculate the maximum possible scroll so you can't scroll past the oldest message
        let max_scroll = total_msgs.saturating_sub(display_count);
        let safe_scroll = game.msg_scroll.min(max_scroll);
        
        // Slice the vector to only show the lines we scrolled to
        let start_idx = total_msgs.saturating_sub(display_count).saturating_sub(safe_scroll);
        let end_idx = (start_idx + display_count).min(total_msgs);
        
        let msg_lines: Vec<Line> = game.messages[start_idx..end_idx].iter()
            .map(|m| Line::from(m.clone()))
            .collect();
            
        let msg_title = format!(" Messages (Scroll: {}) ", safe_scroll);
        let msg_widget = Paragraph::new(msg_lines)
            .block(Block::default().borders(Borders::ALL).title(msg_title));
        f.render_widget(msg_widget, msg_area);
        // Prompt Widget
        let prompt_widget = Paragraph::new(prompt)
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        f.render_widget(prompt_widget, prompt_area);
    }).unwrap();
}
