// Keys on Litematica's menu: arrows through the tabs and the list; Escape
// is left to close the screen; other keys do nothing to the page beneath.
fn litematica_menu_key(
    options: &mut crate::options_ui::OptionsUi,
    key: winit::keyboard::KeyCode,
) -> core::ops::ControlFlow<Vec<Event>> {
    use crate::options_ui::litematica_menu as m;
    if !m::shows(options) {
        return core::ops::ControlFlow::Continue(());
    }
    if !m::key(key) && key == winit::keyboard::KeyCode::Escape {
        return core::ops::ControlFlow::Continue(());
    }
    core::ops::ControlFlow::Break(Vec::new())
}
