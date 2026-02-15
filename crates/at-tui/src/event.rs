// Event handling is currently inlined in main.rs (crossterm::event::poll + read).
// This module exists for future expansion (e.g. async event channel, mouse
// support, resize handling).
//
// The key dispatch logic lives in App::on_key (app.rs).
