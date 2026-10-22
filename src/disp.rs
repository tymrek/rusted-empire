use crate::game::Game;
use crate::models::{Terrain, GameMode, Owner, PieceType, Loc};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};
type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

static TUI_TERMINAL: OnceLock<Mutex<TuiTerminal>> = OnceLock::new();

fn get_terminal() -> std::sync::MutexGuard<'static, TuiTerminal> {
    TUI_TERMINAL.get_or_init(|| {
        let backend = CrosstermBackend::new(std::io::stdout());
        Mutex::new(Terminal::new(backend).unwrap())
    }).lock().unwrap()
}

/// The map rows and columns shown in a viewport of `view_height` x `view_width` squares
/// centred on `center`. On a normal map this is a window clamped to the map. On a torus
/// map (`-t`) the window wraps across the seams and shows only playable squares.
pub fn view_axes(game: &Game, center: Loc, view_height: usize, view_width: usize) -> (Vec<usize>, Vec<usize>) {
    if game.torus && game.rows > 2 && game.cols > 2 {
        let h = view_height.min(game.rows - 2);
        let w = view_width.min(game.cols - 2);
        let rows = (0..h).filter_map(|i| game.offset(center, i as i32 - (h / 2) as i32, 0)).map(|l| l.r).collect();
        let cols = (0..w).filter_map(|i| game.offset(center, 0, i as i32 - (w / 2) as i32)).map(|l| l.c).collect();
        return (rows, cols);
    }
    let start_r = center.r.saturating_sub(view_height / 2).min(game.rows.saturating_sub(view_height));
    let end_r = (start_r + view_height).min(game.rows);
    let start_c = center.c.saturating_sub(view_width / 2).min(game.cols.saturating_sub(view_width));
    let end_c = (start_c + view_width).min(game.cols);
    ((start_r..end_r).collect(), (start_c..end_c).collect())
}

pub fn refresh_display_with_input(game: &Game, prompt: &str) {
    refresh_display_with_input_blinking(game, prompt, None);
}

pub fn refresh_display_with_input_blinking(game: &Game, prompt: &str, active_blink: Option<(Loc, bool)>) {
    let mut terminal = get_terminal();

    terminal.draw(|f| {
        let size = f.size();
        
        f.render_widget(Clear, size);

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

        let upper_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(40),
                Constraint::Length(30),
            ])
            .split(upper_area);

        let map_area = upper_chunks[0];
        let sidebar_area = upper_chunks[1];

        let center_loc = if let Some(vp) = game.viewport_center {
            vp
        } else if game.mode == GameMode::Edit || matches!(game.mode, GameMode::PickDest(_)) {
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


        let (row_coords, col_coords) = view_axes(game, center_loc, view_height, view_width);
        let mut in_rows = vec![false; game.rows];
        let mut in_cols = vec![false; game.cols];
        for &r in &row_coords { in_rows[r] = true; }
        for &c in &col_coords { in_cols[c] = true; }

        // Fog of war as in the original: the player's own units are always shown; enemy
        // units and city owners are shown live only where the player can see right now,
        // and elsewhere as last seen.
        let sight = game.player_sight();
        let mut viewport_pieces: HashMap<Loc, (u8, Owner)> = HashMap::new();
        for (i, p) in game.pieces.iter().enumerate() {
            if p.hits > 0 && in_rows[p.loc.r] && in_cols[p.loc.c] && game.is_visible_with(i, Owner::Player, Some(&sight)) {
                viewport_pieces.entry(p.loc).or_insert((Game::unit_char(p.ptype, p.owner), p.owner));
            }
        }
        for &r in &row_coords {
            for &c in &col_coords {
                let loc = Loc { r, c };
                let remembered = game.player_view.get(r).and_then(|row| row.get(c)).copied().unwrap_or(0);
                if remembered != 0 && !sight[r][c] && !viewport_pieces.contains_key(&loc) {
                    viewport_pieces.insert(loc, (remembered, Owner::Comp));
                }
            }
        }

        let mut viewport_cities: HashMap<Loc, Owner> = HashMap::new();
        for city in game.cities.iter() {
            if in_rows[city.loc.r] && in_cols[city.loc.c] {
                if let Some(owner) = game.known_city_owner(city, Some(&sight)) {
                    viewport_cities.entry(city.loc).or_insert(owner);
                }
            }
        }

        let mut lines = Vec::new();
        for &r in &row_coords {
            let mut spans = Vec::new();
            for &c in &col_coords {
                let loc = Loc { r, c };
                let is_explored = game.player_map[r][c] == 1;

                if !is_explored {
                    spans.push(Span::raw(" "));
                    continue;
                }

                if (game.mode == GameMode::Edit || matches!(game.mode, GameMode::PickDest(_))) && game.edit_cursor == loc {
                    spans.push(Span::styled("█", Style::default().fg(Color::White)));
                    continue;
                }

                let is_blink_target = active_blink.map_or(false, |(b_loc, _)| b_loc == loc);
                let is_blink_off = is_blink_target && !active_blink.unwrap().1;

                let piece_opt = viewport_pieces.get(&loc).copied();
                if let Some((ch, owner)) = piece_opt {
                    if !is_blink_off {
                        let ch_str = (ch as char).to_string();
                        // Units remembered from out of sight are drawn dimmer than live ones.
                        let color = if owner == Owner::Player { Color::Cyan }
                                    else if sight[r][c] { Color::LightRed }
                                    else { Color::Red };
                        let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                        if is_blink_target { style = style.bg(Color::White).fg(Color::Black); }
                        spans.push(Span::styled(ch_str, style));
                        continue;
                    }
                }

                let city_opt = viewport_cities.get(&loc).copied();
                if let Some(city_owner) = city_opt {
                    if !(is_blink_off && piece_opt.is_none()) {
                        let ch_str = match city_owner {
                            Owner::Player => "O",
                            Owner::Comp => "X",
                            Owner::Neutral => "*",
                        };

                        let color = match city_owner {
                            Owner::Player => Color::Green,
                            Owner::Comp => Color::Red,
                            Owner::Neutral => Color::Yellow,
                        };
                        let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                        if is_blink_target { style = style.bg(Color::White).fg(Color::Black); }
                        spans.push(Span::styled(ch_str, style));
                        continue;
                    }
                }

                let terrain_ch = game.map[r][c];
                let mut style = if terrain_ch == Terrain::Land { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Blue) };
                if is_blink_target { style = style.bg(Color::White).fg(Color::Black); }
                
                if terrain_ch == Terrain::Land {
                    spans.push(Span::styled("#", style));
                } else {
                    spans.push(Span::styled(".", style));
                }
            }
            lines.push(Line::from(spans));
        }

        let map_title = format!(" rs Empire | Turn: {} | Mode: {:?} ", game.turn, game.mode);
        let map_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(map_title));
        f.render_widget(map_widget, map_area);

        let player_armies = game.pieces.iter().filter(|p| p.owner == Owner::Player && p.ptype == PieceType::Army).count();
        let player_cities = game.cities.iter().filter(|c| c.owner == Owner::Player).count();
        let enemy_cities = game.cities.iter().filter(|c| game.known_city_owner(c, Some(&sight)) == Some(Owner::Comp)).count();
let mut sidebar_lines = vec![
            Line::from(Span::styled("--- STATUS ---", Style::default().fg(Color::Yellow))),
            Line::from(format!(" My Score: {}", game.player_score)),
            Line::from(format!(" Enemy Score: {}", game.comp_score)),
            Line::from(format!(" Armies: {}", player_armies)),
            Line::from(format!(" My Cities: {}", player_cities)),
            Line::from(format!(" Enemy Cities (known): {}", enemy_cities)),
            Line::from(""),
        ];

        if game.mode == GameMode::Edit {
            sidebar_lines.push(Line::from(Span::styled("--- CURSOR INFO ---", Style::default().fg(Color::Magenta))));
            let loc = game.edit_cursor;
            
            if game.player_map[loc.r][loc.c] == 1 {
                let terrain = if game.map[loc.r][loc.c] == Terrain::Land { "Land" } else { "Sea" };
                sidebar_lines.push(Line::from(format!(" Terrain: {}", terrain)));

                if let Some(city) = game.cities.iter().find(|c| c.loc == loc && game.known_city_owner(c, Some(&sight)).is_some()) {
                    let owner_str = match game.known_city_owner(city, Some(&sight)) {
                        Some(Owner::Player) => "Player",
                        Some(Owner::Comp) => "Enemy",
                        _ => "Neutral",
                    };
                    sidebar_lines.push(Line::from(format!(" City: {}", owner_str)));
                    if city.owner == Owner::Player {
                        sidebar_lines.push(Line::from(format!(" Prod: {:?} ({}/{})", city.prod, city.work, city.prod.build_cost())));
                        
                        if !city.default_orders.is_empty() {
                            sidebar_lines.push(Line::from(" Local Orders:"));
                            for (p, a) in &city.default_orders {
                                sidebar_lines.push(Line::from(format!("  {:?} -> {:?}", p, a)));
                            }
                        } else {
                            sidebar_lines.push(Line::from(" Local Orders: None"));
                        }
                        
                        if let Some((_, act)) = game.default_orders.iter().find(|(p, _)| *p == city.prod) {
                            sidebar_lines.push(Line::from(format!(" Glob Fallback: {:?}", act)));
                        }
                    }
                }
                
                let visible_pieces: Vec<_> = game.pieces.iter().enumerate()
                    .filter(|(i, p)| p.loc == loc && p.hits > 0 && game.is_visible_with(*i, Owner::Player, Some(&sight)))
                    .map(|(_, p)| p)
                    .collect();

                if !visible_pieces.is_empty() {
                    sidebar_lines.push(Line::from(format!(" Units Here: {}", visible_pieces.len())));
                    
                    // Group units by Owner, Type, and Activity for clean UI stacking
                    use std::collections::BTreeMap;
                    let mut unit_groups = BTreeMap::new();
                    for p in &visible_pieces {
                        let key = (p.owner, p.ptype, p.activity);
                        *unit_groups.entry(key).or_insert(0) += 1;
                    }
                    
                    for ((owner, ptype, activity), count) in unit_groups {
                        let owner_str = match owner { Owner::Player => "My", Owner::Comp => "Enemy", Owner::Neutral => "Neut" };
                        sidebar_lines.push(Line::from(format!(" - {}x {} {:?} {:?}", count, owner_str, ptype, activity)));
                    }
                }
            } else {
                sidebar_lines.push(Line::from(" Terrain: Fog of War"));
            }
            sidebar_lines.push(Line::from(""));
        } else if let Some((loc, _)) = active_blink {
            if let Some(piece) = game.pieces.iter().find(|p| p.loc == loc && p.owner == Owner::Player && p.hits > 0) {
                sidebar_lines.push(Line::from(Span::styled("--- ACTIVE UNIT ---", Style::default().fg(Color::Cyan))));
                sidebar_lines.push(Line::from(format!(" Unit: Player {:?}", piece.ptype)));
                sidebar_lines.push(Line::from(format!(" Hits: {}/{}", piece.hits, piece.ptype.max_hits())));
                sidebar_lines.push(Line::from(format!(" Moves: {}/{}", piece.moves_left, piece.ptype.max_moves())));
                if piece.ptype == PieceType::Fighter {
                    sidebar_lines.push(Line::from(format!(" Fuel: {}/{}", piece.fuel, crate::game::FIGHTER_FUEL)));
                } else if piece.ptype == PieceType::Satellite {
                    sidebar_lines.push(Line::from(format!(" Orbit: {}/{} turns", piece.fuel, crate::game::SATELLITE_LIFE)));
                }
                if piece.ptype == PieceType::Transport || piece.ptype == PieceType::Carrier {
                    sidebar_lines.push(Line::from(format!(" Cargo: {} units", piece.cargo.len())));
                } else if !piece.cargo.is_empty() {
                    sidebar_lines.push(Line::from(format!(" Cargo: {} units", piece.cargo.len())));
                }
                sidebar_lines.push(Line::from(""));
            }
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

        let total_msgs = game.messages.len();
        let display_count = 5; 
        
        let max_scroll = total_msgs.saturating_sub(display_count);
        let safe_scroll = game.msg_scroll.min(max_scroll);
        
        let start_idx = total_msgs.saturating_sub(display_count).saturating_sub(safe_scroll);
        let end_idx = (start_idx + display_count).min(total_msgs);
        
        let msg_lines: Vec<Line> = game.messages[start_idx..end_idx].iter()
            .map(|m| Line::from(m.clone()))
            .collect();
            
        let msg_title = format!(" Messages (Scroll: {}) ", safe_scroll);
        let msg_widget = Paragraph::new(msg_lines)
            .block(Block::default().borders(Borders::ALL).title(msg_title));
        f.render_widget(msg_widget, msg_area);
        let prompt_widget = Paragraph::new(prompt)
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        f.render_widget(prompt_widget, prompt_area);
    }).unwrap();

    #[cfg(debug_assertions)]
        {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open("ai_metrics.csv") {
                let comp_cities = game.cities.iter().filter(|c| c.owner == Owner::Comp).count();
                let comp_units = game.pieces.iter().filter(|p| p.owner == Owner::Comp && p.hits > 0).count();
                let _ = writeln!(file, "{},{},{}", game.turn, comp_cities, comp_units);
            }
        }
}
