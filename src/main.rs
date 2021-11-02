mod pipe;

use crate::pipe::read_offer;
use calloop::EventLoop;
use calloop::LoopHandle;
use calloop::LoopSignal;
use smithay_client_toolkit::WaylandSource;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::DispatchData;
use wayland_client::Display;
use wayland_client::GlobalManager;
use wayland_client::Main;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_device_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_device_v1::ZwlrDataControlDeviceV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1::ZwlrDataControlSourceV1;

// TODO: It's possible that "Cell" works here too...?

// This is a reference-counted, mutable hashmap. It contains mime-types as
// keys, and raw binary blobs (e.g.: utf8 strings, raw jpegs, etc) as values.
type MimeTypes = Rc<RefCell<HashMap<String, Option<Vec<u8>>>>>;

#[derive(Debug, Clone, Copy)]
enum Selection {
    Primary,   // Selected text.
    Clipboard, // Ctrl+C.
}

#[derive(Debug)]
pub struct DataOffer {
    mime_types: MimeTypes,
    selection: RefCell<Option<Selection>>,
}

impl DataOffer {
    fn new() -> DataOffer {
        DataOffer {
            mime_types: Rc::new(RefCell::new(HashMap::new())),
            selection: RefCell::new(None),
        }
    }
}

#[derive(Debug)]
enum SelectionState {
    Free,
    Ours(MimeTypes),
    Client { data_offer_id: u32 },
}

#[derive(Debug)]
pub struct LoopData {
    signal: LoopSignal,
    manager: Main<ZwlrDataControlManagerV1>,
    device: Main<ZwlrDataControlDeviceV1>,
    primary: SelectionState,
    clipboard: SelectionState,
}

impl LoopData {
    fn new(
        signal: LoopSignal,
        manager: Main<ZwlrDataControlManagerV1>,
        device: Main<ZwlrDataControlDeviceV1>,
    ) -> LoopData {
        LoopData {
            signal,
            manager,
            device,
            primary: SelectionState::Free,
            clipboard: SelectionState::Free,
        }
    }

    fn take_selection(
        &mut self,
        selection: Selection,
        data: &MimeTypes,
        source: &ZwlrDataControlSourceV1,
    ) {
        let new_state = SelectionState::Ours(Rc::clone(data));

        match selection {
            Selection::Primary => {
                self.primary = new_state;
                self.device.set_primary_selection(Some(source));
            }
            Selection::Clipboard => {
                self.clipboard = new_state;
                self.device.set_selection(Some(source));
            }
        }
    }

    /// Record that a client has taken a selection.
    fn set_data_offer_for_selection(&mut self, selection: Selection, data_offer_id: u32) {
        let new_state = SelectionState::Client { data_offer_id };

        match selection {
            Selection::Primary => self.primary = new_state,
            Selection::Clipboard => self.clipboard = new_state,
        }
    }

    /// Indicates whether a data_source owns a selection.
    fn is_selection_owned_by_client(&self, selection: Selection, id: u32) -> bool {
        let state = match selection {
            Selection::Primary => &self.primary,
            Selection::Clipboard => &self.clipboard,
        };

        match state {
            SelectionState::Client { data_offer_id } => *data_offer_id == id,
            _ => false,
        }
    }

    fn selection_lost(&mut self, selection: Selection) {
        match selection {
            Selection::Primary => self.primary = SelectionState::Free,
            Selection::Clipboard => self.clipboard = SelectionState::Free,
        }
    }

    fn is_selection_ours(&self, selection: Selection) -> bool {
        let state = match selection {
            Selection::Primary => &self.primary,
            Selection::Clipboard => &self.clipboard,
        };
        matches!(state, SelectionState::Ours(_))
    }

    fn get_selection_data(&self, selection: Selection) -> Option<MimeTypes> {
        let state = match selection {
            Selection::Primary => &self.primary,
            Selection::Clipboard => &self.clipboard,
        };

        return match state {
            SelectionState::Ours(mime_types) => Some(Rc::clone(&mime_types)),
            _ => None,
        };
    }
}

/// Handles events from the data_offer.
/// These events describe the data being offered by an owner of the clipboard.
fn handle_data_offer_events(
    main: Main<ZwlrDataControlOfferV1>,
    ev: zwlr_data_control_offer_v1::Event,
    _dispatch_data: DispatchData,
) {
    match ev {
        zwlr_data_control_offer_v1::Event::Offer { mime_type } => {
            // TODO: Report this crash upstream:
            //
            // Apparently, trying to read from the ControlOffer at this point has mixed results.
            // If the selection comes from alacritty it works.
            // If the selection comes from firefox it doesn't.

            let user_data = main
                .as_ref()
                .user_data()
                .get::<DataOffer>()
                .expect("user_data is of type DataOffer");

            user_data.mime_types.borrow_mut().insert(mime_type, None);
        }
        _ => unreachable!(),
    }
}

// Handle a selection being taken by another client.
fn handle_selection_taken(
    id: Option<ZwlrDataControlOfferV1>,
    selection: Selection,
    loop_data: &mut LoopData,
    handle: &LoopHandle<LoopData>,
) {
    if loop_data.is_selection_ours(selection) {
        // This is just the compositor notifying us of an event we created;
        // We can ignore it since we already own this selection.
        println!("We already own {:?}, escaping.", selection);
        return;
    }

    // This is sent AFTER the offers, and indicates that all the mime types have been
    // specified. The id is that of the objet gotten via DataOffer.

    match id.as_ref() {
        Some(data_offer) => {
            let user_data = data_offer
                .as_ref()
                .user_data()
                .get::<DataOffer>()
                .expect("user_data is of type DataOffer");
            user_data.selection.replace(Some(selection));

            // Keep a record of which remote dataoffer owns the selection.
            loop_data.set_data_offer_for_selection(selection, data_offer.as_ref().id());

            read_offer(data_offer, handle, user_data);
        }
        // Empty means that the selection is owned by "no one".
        None => loop_data.selection_lost(selection),
    };
}

/// Handles events from the data_device.
/// These events are basically new offers my clients that are taking ownership
/// of the clipboard.
fn handle_data_device_events(
    data_device: Main<ZwlrDataControlDeviceV1>,
    ev: zwlr_data_control_device_v1::Event,
    loop_data: &mut LoopData,
    handle: &LoopHandle<LoopData>,
) {
    match ev {
        zwlr_data_control_device_v1::Event::DataOffer { id: data_offer } => {
            // Maybe HashMap makes more sense and we can store the content?
            data_offer.as_ref().user_data().set(DataOffer::new);
            data_offer.quick_assign(handle_data_offer_events)
        }
        zwlr_data_control_device_v1::Event::Selection { id } => {
            handle_selection_taken(id, Selection::Clipboard, loop_data, handle);
        }
        zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
            handle_selection_taken(id, Selection::Primary, loop_data, handle);
        }
        zwlr_data_control_device_v1::Event::Finished => {
            data_device.destroy();
            // XXX: What happens if we're still reading here...?
        }
        _ => unreachable!(),
    }
}

fn main() {
    let display = Display::connect_to_env().expect("display is valid");
    let mut event_queue = display.create_event_queue();
    let attached_display = (*display).clone().attach(event_queue.token());
    let globals = GlobalManager::new(&attached_display);

    // Make a synchronized roundtrip to the wayland server.
    //
    // When this returns it must be true that the server has already
    // sent us all available globals.
    event_queue
        .sync_roundtrip(&mut (), |_, _, _| unreachable!())
        .expect("round trip to compositor");

    let seat = globals
        .instantiate_exact::<WlSeat>(1)
        .expect("get seat from compositor");
    seat.quick_assign(|_, _, _| {}); // Ignore all events for the seat.

    event_queue
        .sync_roundtrip(&mut (), |_, _, _| unreachable!())
        .expect("round trip to compositor");

    let manager = globals
        .instantiate_exact::<ZwlrDataControlManagerV1>(2)
        .expect("compositor supports wlr-data-control-unstable-v1");

    let mut event_loop = EventLoop::<LoopData>::try_new().expect("initialise event loop");

    let data_device = manager.get_data_device(&seat);
    // This will set up handlers to listen to selection ("copy") events.
    // It'll also handle the initially set selections.
    let handle = event_loop.handle();
    data_device.quick_assign(move |data_device, event, mut data| {
        let loop_data = data
            .get::<LoopData>()
            .expect("loop data is of type LoopData");
        handle_data_device_events(data_device, event, loop_data, &handle)
    });

    // Send all pending messages to the compositor.
    // Doesn't fetch events -- we'll get those after the event loop starts.
    display
        .flush()
        .expect("send initialisation to the compositor");

    // TODO: create a "Ping" source for showing notifications.
    // TODO: trigger a notification on paste.

    WaylandSource::new(event_queue)
        .quick_insert(event_loop.handle())
        .expect("add wayland connection to the event loop");

    eprintln!("Starting event loop...");
    event_loop
        .run(
            std::time::Duration::from_millis(1),
            &mut LoopData::new(event_loop.get_signal(), manager, data_device),
            |_| {},
        )
        .expect("run the event loop");
}
