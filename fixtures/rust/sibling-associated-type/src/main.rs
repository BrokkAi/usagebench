pub mod state;

use state::AppState;

pub fn app_with_environment() {
    let _ = AppState::with_environment();
}
