mod app;
mod handlers;
mod ui;
mod util;

use rspotify::spotify::client::Spotify;
use rspotify::spotify::oauth2::{SpotifyClientCredentials, SpotifyOAuth};
use rspotify::spotify::util::get_token;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use termion::cursor::Goto;
use termion::event::Key;
use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::Terminal;

use app::{ActiveBlock, App};
use util::{Event, Events};

const SCOPES: [&str; 6] = [
    "user-modify-playback-state",
    "user-read-playback-state",
    "user-read-private",
    "user-read-currently-playing",
    "playlist-read-private",
    "user-library-read",
];

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ClientConfig {
    client_id: String,
    client_secret: String,
}

fn main() -> Result<(), failure::Error> {
    let client_config = util::get_config()?;

    // Start authorization with spotify
    let mut oauth = SpotifyOAuth::default()
        .client_id(&client_config.client_id)
        .client_secret(&client_config.client_secret)
        // TODO: use a webpage
        .redirect_uri("http://localhost:8888/callback")
        .scope(&SCOPES.join(" "))
        .build();
    match get_token(&mut oauth) {
        Some(token_info) => {
            // Terminal initialization
            let stdout = io::stdout().into_raw_mode()?;
            let stdout = MouseTerminal::from(stdout);
            let stdout = AlternateScreen::from(stdout);
            let backend = TermionBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;
            terminal.hide_cursor()?;

            let events = Events::new();

            // Initialise app state
            let mut app = App::new();

            let client_credential = SpotifyClientCredentials::default()
                .token_info(token_info)
                .build();

            let spotify = Spotify::default()
                .client_credentials_manager(client_credential)
                .build();

            app.spotify = Some(spotify);

            // Now that spotify is ready, check if the user has already selected a device_id to
            // play music on, if not send them to the device selection view
            match app.get_cached_device_token() {
                Ok(device_id) => {
                    app.device_id = Some(device_id);
                }
                Err(_e) => {
                    app.handle_get_devices();
                }
            }

            if let Some(spotify) = &app.spotify {
                let playlists = spotify.current_user_playlists(app.large_search_limit, None);

                match playlists {
                    Ok(p) => {
                        app.playlists = Some(p);
                        // Select the first playlist
                        app.selected_playlist_index = Some(0);
                    }
                    Err(e) => {
                        app.handle_error(e);
                    }
                };
            }

            app.get_current_playback();

            loop {
                let current_route = app.get_current_route();
                terminal.draw(|mut f| match current_route.active_block {
                    ActiveBlock::HelpMenu => {
                        ui::draw_help_menu(&mut f);
                    }
                    ActiveBlock::Error => {
                        ui::draw_error_screen(&mut f, &app);
                    }
                    ActiveBlock::SelectDevice => {
                        ui::draw_device_list(&mut f, &app);
                    }
                    _ => {
                        ui::draw_main_layout(&mut f, &app);
                    }
                })?;

                if current_route.active_block == ActiveBlock::Input {
                    match terminal.show_cursor() {
                        Ok(_r) => {}
                        Err(_e) => {}
                    };
                } else {
                    match terminal.hide_cursor() {
                        Ok(_r) => {}
                        Err(_e) => {}
                    };
                }

                // Put the cursor back inside the input box
                write!(
                    terminal.backend_mut(),
                    "{}",
                    Goto(4 + app.input_cursor_position, 4)
                )?;

                // stdout is buffered, flush it to see the effect immediately when hitting backspace
                io::stdout().flush().ok();

                match events.next()? {
                    Event::Input(key) => {
                        match key {
                            // Global key presses
                            Key::Char('q') | Key::Ctrl('c') => break,
                            Key::Char('-') => {
                                // Navigate back one step
                                app.pop_navigation_stack();
                            }
                            _ => handlers::handle_app(&mut app, key),
                        }
                    }
                    Event::Tick => {
                        app.update_on_tick();
                    }
                }
            }
        }
        None => println!("\nSpotify auth failed"),
    }

    Ok(())
}
