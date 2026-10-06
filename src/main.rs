use ag_blackout::ui::{app, terminal};

fn main() -> std::io::Result<()> {
    let _guard = terminal::TerminalGuard::enter()?;
    let mut term = terminal::init()?;
    app::run(&mut term)
}
