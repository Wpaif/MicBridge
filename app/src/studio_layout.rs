//! Colunas independentes: cada painel usa sua altura natural, com largura limitada.
use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

pub fn install(
    bin: &adw::BreakpointBin,
    workspace: &gtk::Box,
    clamp: &adw::Clamp,
    sections: [gtk::Widget; 8],
) {
    let columns = std::array::from_fn(|_| {
        gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .hexpand(true)
            .valign(gtk::Align::Start)
            .build()
    });
    let layout = Rc::new(Columns {
        workspace: workspace.clone(),
        columns,
        sections,
        count: Cell::new(0),
    });
    let medium =
        adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 1240px").unwrap());
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 800px").unwrap());
    for (breakpoint, width, margin) in [(&medium, 1040, 24), (&narrow, 584, 12)] {
        breakpoint.add_setter(clamp, "maximum-size", Some(&width.to_value()));
        breakpoint.add_setter(clamp, "tightening-threshold", Some(&width.to_value()));
        breakpoint.add_setter(workspace, "margin-start", Some(&margin.to_value()));
        breakpoint.add_setter(workspace, "margin-end", Some(&margin.to_value()));
    }
    bin.add_breakpoint(medium.clone());
    bin.add_breakpoint(narrow.clone());
    let update = {
        let layout = layout.clone();
        move |bin: &adw::BreakpointBin| {
            let active = bin.current_breakpoint();
            let count = if active.as_ref() == Some(&narrow) {
                1
            } else if active.as_ref() == Some(&medium) {
                2
            } else {
                3
            };
            let focus = bin
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok())
                .and_then(|w| gtk::prelude::GtkWindowExt::focus(&w));
            if layout.arrange(count) {
                if let Some(focus) = focus {
                    focus.grab_focus();
                }
            }
        }
    };
    update(bin);
    bin.connect_current_breakpoint_notify(update);
}
struct Columns {
    workspace: gtk::Box,
    columns: [gtk::Box; 3],
    /// Perfil, limpeza, timbre, dinâmica, volume, teste, retorno, nota.
    sections: [gtk::Widget; 8],
    count: Cell<usize>,
}
impl Columns {
    fn arrange(&self, count: usize) -> bool {
        if self.count.replace(count) == count {
            return false;
        }
        for section in &self.sections {
            if let Some(parent) = section.parent() {
                parent
                    .downcast::<gtk::Box>()
                    .expect("painel em coluna")
                    .remove(section);
            }
        }
        while let Some(child) = self.workspace.first_child() {
            self.workspace.remove(&child);
        }
        let groups: &[&[usize]] = match count {
            3 => &[&[0, 1, 2], &[3, 4, 7], &[5, 6]],
            2 => &[&[0, 1, 2, 3, 4, 7], &[5, 6]],
            _ => &[&[0, 1, 2, 3, 4, 5, 6, 7]],
        };
        for (column, indices) in self.columns.iter().zip(groups) {
            self.workspace.append(column);
            for index in *indices {
                column.append(&self.sections[*index]);
            }
        }
        true
    }
}
