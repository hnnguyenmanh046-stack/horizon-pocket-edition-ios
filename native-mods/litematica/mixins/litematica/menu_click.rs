// Clicks on Litematica's menu.
fn litematica_menu_click(
    options: &mut crate::options_ui::OptionsUi,
    x: f32,
    y: f32,
    drag: bool,
    right: bool,
) -> core::ops::ControlFlow<Vec<Event>> {
    use crate::options_ui::litematica_menu as m;
    if !m::shows(options) {
        return core::ops::ControlFlow::Continue(());
    }
    if !right && !drag {
        m::click(x, y);
    }
    core::ops::ControlFlow::Break(Vec::new())
}
