// Typing on Litematica's menu goes nowhere (it has no text boxes), so it
// doesn't reach the page underneath.
fn litematica_menu_type(options: &mut crate::options_ui::OptionsUi, text: &str) -> core::ops::ControlFlow<()> {
    use crate::options_ui::litematica_menu as m;
    let _ = text;
    if !m::shows(options) {
        return core::ops::ControlFlow::Continue(());
    }
    core::ops::ControlFlow::Break(())
}
