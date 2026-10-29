//! rusted-empire in the browser.
//!
//! This crate compiles the game's own sources from `../src` unchanged, with the browser
//! front end (`web.rs`) in place of the terminal. The game loop (`app::play`) runs in a
//! Web Worker; `www/` holds the page that draws the screen and forwards the keyboard and
//! mouse.
//!
//! Some of the shared code (the terminal, the headless rig's helpers) has no use here.
#![allow(dead_code)]

#[path = "../../src/ai.rs"]
mod ai;
#[path = "../../src/app.rs"]
mod app;
#[path = "../../src/disp.rs"]
mod disp;
#[path = "../../src/game.rs"]
mod game;
#[path = "../../src/keys.rs"]
mod keys;
#[path = "../../src/models.rs"]
mod models;
#[path = "../../src/ui.rs"]
mod ui;
#[path = "../../src/view.rs"]
mod view;

mod web;

pub use web::run;
