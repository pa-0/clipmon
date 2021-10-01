mod pipe;

use crate::pipe::read_offer;
use calloop::EventLoop;
use calloop::LoopHandle;
use calloop::LoopSignal;
use smithay_client_toolkit::WaylandSource;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use std::rc::Rc;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::DispatchData;
use wayland_client::Display;
use wayland_client::GlobalManager;
use wayland_client::Main;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_device_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1::ZwlrDataControlSourceV1;

#[derive(Debug)]
pub struct ControlOfferUserData {
    mime_types: Rc<RefCell<HashMap<String, Option<Vec<u8>>>>>,
    is_primary: RefCell<bool>,
    is_clipboard: RefCell<bool>,
}

impl ControlOfferUserData {
    fn new() -> ControlOfferUserData {
        ControlOfferUserData {
            mime_types: Rc::new(RefCell::new(HashMap::new())),
            is_primary: RefCell::new(false),
            is_clipboard: RefCell::new(false),
        }
    }
}

// TODO: I probably want a SeatUserData to keep data_offers around.

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

            let user_data = main
                .as_ref()
                .user_data()
                .get::<ControlOfferUserData>()
                .unwrap();

            user_data.mime_types.borrow_mut().insert(mime_type, None);
        }
        _ => unreachable!(),
    }
}

/// Handle events on the sources we create.
fn handle_source_events(
    data_source: Main<ZwlrDataControlSourceV1>,
    ev: zwlr_data_control_source_v1::Event,
    _dispatch_data: DispatchData,
) {
    match ev {
        // Someone is trying to paste a selection.
        zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
            println!("Someone asked us to send: {}", mime_type);
            {
                let mut file = unsafe { File::from_raw_fd(fd) };
                match write!(file, "Helo!") {
                    Ok(()) => {
                        println!("sent clip!");
                    }
                    Err(err) => {
                        eprintln!("error sending clip: {:?}!", err);
                    }
                };
            }
        }
        // Our selection has been cancelled.
        zwlr_data_control_source_v1::Event::Cancelled {} => {
            // TODO: Drop any references to this.
            data_source.destroy();
            println!("We've been cancelled");
        }
        _ => unreachable!(),
    }
}

/// Handles events from the data_device.
/// These events are basically new offers my clients that are taking ownership
/// of the clipboard.
fn handle_data_device_events(
    data_device: Main<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1>,
    ev: zwlr_data_control_device_v1::Event,
    handle: &LoopHandle<LoopSignal>,
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
                .set(move || ControlOfferUserData::new());
            data_offer.quick_assign(handle_data_offer_events)
        }
        zwlr_data_control_device_v1::Event::Selection { id } => {
            // CLIPBOARD selection (ctrl+c)

            // This is sent AFTER the offers, and indicates that all the mime types have been
            // specified. The id is that of the objet gotten via DataOffer.

            let data_offer = match id.as_ref() {
                Some(data_offer) => data_offer,
                None => {
                    // This should not really happen.
                    // We copy clipboard data immediately, and then expose it ourselves, so
                    // applications should seldom UNSET any selection.
                    eprintln!("The CLIPBOARD selection has been dropped.");
                    return;
                }
            };

            let user_data = data_offer
                .as_ref()
                .user_data()
                .get::<ControlOfferUserData>()
                .unwrap();

            user_data.is_clipboard.replace_with(|_| true);

            read_offer(&data_offer, handle);

            // TODO: if this is null, expire the previous offer
            eprintln!("selection: {:?}", data_offer);
        }
        zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
            // PRIMARY selection. Details are the same as above.
            let data_offer = match id.as_ref() {
                Some(data_offer) => data_offer,
                None => {
                    // This should not really happen.
                    // We copy clipboard data immediately, and then expose it ourselves, so
                    // applications should seldom UNSET any selection.
                    eprintln!("The PRIMARY selection has been dropped.");
                    return;
                }
            };

            let user_data = data_offer
                .as_ref()
                .user_data()
                .get::<ControlOfferUserData>()
                .unwrap();

            user_data.is_primary.replace_with(|_| true);

            // TODO: if this is null, expire the previous offer
            eprintln!("primary: {:?}", id)
        }
        zwlr_data_control_device_v1::Event::Finished => {
            // TODO: Drop references to this object.
            data_device.destroy();
            eprintln!("Finished")
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
        EventLoop::<LoopSignal>::try_new().expect("Failed to initialise event loop.");

    let data_device = manager.get_data_device(&seat);
    // This will set up handlers to listen to selection ("copy") events.
    // It'll also handle the initially set selections.
    let handle = event_loop.handle();
    data_device.quick_assign(move |data_source, event, _| {
        handle_data_device_events(data_source, event, &handle)
    });

    // Send all pending messages to the compositor.
    // Doesn't fetch events -- we'll get those after the event loop starts.
    display
        .flush()
        .expect("Failed to send initialisation to compositor");

    WaylandSource::new(event_queue)
        .quick_insert(event_loop.handle())
        .unwrap();
    let mut shared_data = event_loop.get_signal();

    println!("Starting event loop...");
    event_loop
        .run(
            std::time::Duration::from_millis(1),
            &mut shared_data,
            |_| {},
        )
        .expect("An error occurred during the event loop!");
}

// // XXX: Testing. This aquires the CLIPBOARD selection.
// let data_source = manager.create_data_source();
// data_source.quick_assign(handle_source_events);
// data_source.offer("text/plain;charset=utf-8".to_string());
// data_source.offer("text/html".to_string());
// data_device.set_selection(Some(&data_source));
// // data_device.set_primary_selection(Some(&data_source));
