//! The question asked when no screen reader is running.
//!
//! Nothing in here is spoken. The person being asked has no screen reader,
//! and speaking the question through a system voice is the behaviour this
//! dialog exists to stop.

use wxdragon::prelude::*;

use crate::ui::main_frame::MainFrame;

/// Ask once whether to speak progress through a system voice, and save it.
///
/// Does nothing when a screen reader is active, speech is already off, or
/// the question has been answered before. No is the default, so Enter and
/// closing the window both leave speech off.
pub fn offer(frame: &MainFrame) {
    let needed = {
        let state = frame.state.borrow();
        state.settings.speech.offer_spoken_progress(
            state.announcer.is_screen_reader(),
            state.announcer.has_backend(),
        )
    };
    if !needed {
        return;
    }

    let yes = ask(&frame.frame);
    {
        let mut state = frame.state.borrow_mut();
        state.settings.speech.answer_spoken_progress(yes);
        state.save_settings();
        state.apply_settings();
    }
    if yes {
        frame.log("Spoken progress is on.");
    } else {
        frame
            .log("Spoken progress is off. A screen reader will still be used when one is running.");
    }
}

/// Yes/no question whose default button is No.
fn ask(parent: &dyn WxWidget) -> bool {
    let dialog = Dialog::builder(parent, "Spoken progress")
        .with_size(520, 200)
        .with_style(DialogStyle::DefaultDialogStyle)
        .build();

    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    let text = StaticText::builder(&dialog)
        .with_label(
            "No screen reader is running. Portkey Drop can speak transfer progress through a \
             system voice. Turn spoken progress messages on?",
        )
        .build();
    sizer.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 12);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let yes = Button::builder(&dialog)
        .with_id(ID_YES)
        .with_label("&Yes")
        .build();
    let no = Button::builder(&dialog)
        .with_id(ID_NO)
        .with_label("&No")
        .build();
    yes.set_name("Yes");
    no.set_name("No");
    no.set_default();
    buttons.add(&yes, 0, SizerFlag::All, 4);
    buttons.add(&no, 0, SizerFlag::All, 4);
    sizer.add_sizer(&buttons, 0, SizerFlag::AlignRight | SizerFlag::All, 8);

    dialog.set_sizer(sizer, true);
    let answer = dialog.show_modal();
    dialog.destroy();
    answer == ID_YES
}
