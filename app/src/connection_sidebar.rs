//! Conexão fixa no desktop e diálogo nativo quando falta largura ou altura.
use adw::prelude::*;
use std::rc::Rc;

pub fn install(
    window: &adw::ApplicationWindow,
    header: &adw::HeaderBar,
    host: &gtk::Box,
    rail: &adw::Clamp,
    panel: &adw::PreferencesGroup,
    meters: &gtk::Box,
) {
    let button = gtk::Button::builder()
        .name("connection-toggle")
        .icon_name("network-wireless-symbolic")
        .tooltip_text("Conexão")
        .visible(false)
        .build();
    button.update_property(&[gtk::accessible::Property::Label("Conexão")]);
    header.pack_start(&button);
    let controller = Rc::new(Sidebar {
        window: window.downgrade(),
        host: host.clone(),
        panel: panel.clone(),
        dialog: glib::WeakRef::new(),
    });
    let compact = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 1180px or max-height: 680px").unwrap(),
    );
    let phone = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 600px").unwrap());
    for breakpoint in [&compact, &phone] {
        breakpoint.add_setter(rail, "visible", Some(&false.to_value()));
        breakpoint.add_setter(&button, "visible", Some(&true.to_value()));
    }
    phone.add_setter(
        meters,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    phone.add_setter(meters, "spacing", Some(&6i32.to_value()));
    window.add_breakpoint(compact);
    window.add_breakpoint(phone);
    let c = controller.clone();
    button.connect_clicked(move |_| c.open());
    window.connect_current_breakpoint_notify(move |window| {
        if window.current_breakpoint().is_none() {
            if let Some(dialog) = controller.dialog.upgrade() {
                dialog.force_close();
            }
        }
    });
}
struct Sidebar {
    window: glib::WeakRef<adw::ApplicationWindow>,
    host: gtk::Box,
    panel: adw::PreferencesGroup,
    dialog: glib::WeakRef<adw::Dialog>,
}
impl Sidebar {
    fn open(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if let Some(dialog) = self.dialog.upgrade() {
            dialog.present(Some(&window));
            return;
        }
        let dialog = adw::Dialog::builder()
            .title("Conexão")
            .content_width(400)
            .content_height(480)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        self.host.remove(&self.panel);
        content.append(&self.panel);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scroll));
        dialog.set_child(Some(&toolbar));
        self.dialog.set(Some(&dialog));
        let c = self.clone();
        dialog.connect_closed(move |_| {
            content.remove(&c.panel);
            c.host.append(&c.panel);
            c.dialog.set(None::<&adw::Dialog>);
        });
        dialog.present(Some(&window));
    }
}
