//! SEEK's terminal UI (SPEC 4.1, 10).
//!
//! The UI follows The Elm Architecture: Model, Message, Update, View. One loop reads the crossterm
//! event stream, a 100 ms tick and the agent's events. `Update` is pure; side effects are
//! commands.

pub mod app;
pub mod footer;
pub mod input;
pub mod logo;
pub mod logo_bitmap;
pub mod picker;
pub mod prompt_bar;
pub mod run;
pub mod sidebar;
pub mod spinner;
pub mod theme;
pub mod transcript;

pub use app::{App, Command, Message, Overlay, Status};
pub use footer::{Footer, ModelChip, PermissionMode};
pub use input::Input;
pub use picker::{Picker, PickerItem};
pub use sidebar::Sidebar;
pub use spinner::Spinner;
pub use theme::Theme;
pub use transcript::Row;
