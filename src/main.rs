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
pub struct LoopData {
    signal: LoopSignal,
    manager: Main<ZwlrDataControlManagerV1>,
    device: Main<ZwlrDataControlDeviceV1>,
    primary: RefCell<Option<MimeTypes>>,
    clipboard: RefCell<Option<MimeTypes>>,
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
            primary: RefCell::new(None),
            clipboard: RefCell::new(None),
        }
    }

    fn take_selection(
        &self,
        selection: Selection,
        data: &MimeTypes,
        source: &ZwlrDataControlSourceV1,
    ) {
        match selection {
            Selection::Primary => {
                self.primary.replace(Some(Rc::clone(data)));
                self.device.set_primary_selection(Some(source));
            }
            Selection::Clipboard => {
                self.clipboard.replace(Some(Rc::clone(data)));
                self.device.set_selection(Some(source));
            }
        }
    }

    fn selection_lost(&self, selection: Selection) {
        match selection {
            Selection::Primary => {
                self.primary.replace(None);
            }
            Selection::Clipboard => {
                self.clipboard.replace(None);
            }
        }
    }

    fn is_selection_ours(&self, selection: Selection) -> bool {
        return match selection {
            Selection::Primary => self.primary.borrow().is_some(),
            Selection::Clipboard => self.clipboard.borrow().is_some(),
        };
    }

    fn get_selection_data(&self, selection: Selection) -> Option<MimeTypes> {
        let mime_types = match selection {
            Selection::Primary => &self.primary,
            Selection::Clipboard => &self.clipboard,
        };

        mime_types.borrow().as_ref().map(|data| Rc::clone(data))
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
            println!("got offer {:?}", mime_type);

            // TODO: Report this crash upstream:
            //
            // Apparently, trying to read from the ControlOffer at this point has mixed results.
            // If the selection comes from alacritty it works.
            // If the selection comes from firefox it doesn't.

            let user_data = main.as_ref().user_data().get::<DataOffer>().unwrap();

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
        println!("I have {:?}, escaping", selection);
        return;
    }

    // This is sent AFTER the offers, and indicates that all the mime types have been
    // specified. The id is that of the objet gotten via DataOffer.

    let data_offer = match id.as_ref() {
        Some(data_offer) => data_offer,
        None => {
            // This should not really happen.
            // We copy clipboard data immediately, and then expose it ourselves, so
            // applications should seldom UNSET any selection.
            eprintln!("The {:?} selection has been dropped.", selection);
            return;
        }
    };

    let user_data = data_offer.as_ref().user_data().get::<DataOffer>().unwrap();
    user_data.selection.replace(Some(selection));

    read_offer(data_offer, handle, user_data);

    eprintln!("Selection taken by client: {:?}.", selection);
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
            // This means the offer is different from the previous one, and we can flush that
            // previous one.
            //
            // We probably want to create an object to represent this dataoffer, ane associate all
            // the wlr_data_control_offer.offer to it.
            println!("DataOffer: {:?}", data_offer);

            // Maybe HashMap makes more sense and we can store the content?
            data_offer
                .as_ref()
                .user_data()
                .set(DataOffer::new);
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
    let display = Display::connect_to_env().unwrap();
    let mut event_queue = display.create_event_queue();
    let attached_display = (*display).clone().attach(event_queue.token());
    let globals = GlobalManager::new(&attached_display);

    // Make a synchronized roundtrip to the wayland server.
    //
    // When this returns it must be true that the server has already
    // sent us all available globals.
    event_queue
        .sync_roundtrip(&mut (), |_, _, _| unreachable!())
        .unwrap();

    let seat = match globals.instantiate_exact::<WlSeat>(1) {
        Ok(main) => main,
        Err(err) => {
            eprintln!("Failed to get current seat.");
            panic!("{}", err);
        }
    };
    seat.quick_assign(|_, _, _| {}); // Ignore all events for the seat.

    event_queue
        .sync_roundtrip(&mut (), |_, _, _| unreachable!())
        .unwrap();

    let manager = match globals.instantiate_exact::<ZwlrDataControlManagerV1>(2) {
        Err(err) => {
            eprintln!("Compositor doesn't support wlr-data-control-unstable-v1.");
            panic!("{}", err);
        }
        Ok(res) => res,
    };

    let mut event_loop =
        EventLoop::<LoopData>::try_new().expect("Failed to initialise event loop.");

    let data_device = manager.get_data_device(&seat);
    // This will set up handlers to listen to selection ("copy") events.
    // It'll also handle the initially set selections.
    let handle = event_loop.handle();
    data_device.quick_assign(move |data_device, event, mut data| {
        let loop_data = data.get::<LoopData>().unwrap();
        handle_data_device_events(data_device, event, loop_data, &handle)
    });

    // Send all pending messages to the compositor.
    // Doesn't fetch events -- we'll get those after the event loop starts.
    display
        .flush()
        .expect("Failed to send initialisation to the compositor.");

    // TODO: create a speical source for showing notifications.
    // TODO: trigger a notification on paste.

    WaylandSource::new(event_queue)
        .quick_insert(event_loop.handle())
        .expect("Failed to add wayland connection to the event loop.");

    eprintln!("Starting event loop...");
    event_loop
        .run(
            std::time::Duration::from_millis(1),
            &mut LoopData::new(event_loop.get_signal(), manager, data_device),
            |_| {},
        )
        .expect("An error occurred during the event loop!");
}
