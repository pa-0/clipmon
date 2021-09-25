use std::cell::RefCell;
use std::collections::HashSet;
// use std::fs::File;
// use std::os::unix::io::FromRawFd;
// use std::os::unix::io::IntoRawFd;
// use std::os::unix::io::RawFd;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::DispatchData;
use wayland_client::Display;
use wayland_client::GlobalManager;
use wayland_client::Main;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_device_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

#[derive(Debug)]
pub struct ControlOfferUserData {
    mime_types: RefCell<HashSet<String>>,
    is_primary: RefCell<bool>,
    is_clipboard: RefCell<bool>,
}

impl ControlOfferUserData {
    fn new() -> ControlOfferUserData {
        ControlOfferUserData {
            mime_types: RefCell::new(HashSet::<String>::new()),
            is_primary: RefCell::new(false),
            is_clipboard: RefCell::new(false),
        }
    }
}

fn handle_data_offer_events(
    main: Main<ZwlrDataControlOfferV1>,
    ev: zwlr_data_control_offer_v1::Event,
    dispatch_data: DispatchData,
) {
    match ev {
        zwlr_data_control_offer_v1::Event::Offer { mime_type } => {
            println!(
                "got offer {:?}, dispatch_data: {:?}",
                mime_type, dispatch_data
            );

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

            user_data.mime_types.borrow_mut().insert(mime_type);
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

    let seat = globals.instantiate_exact::<WlSeat>(1).unwrap();

    // Once you have a seat, the compositor will send any events for this object.
    // We don't really care about them, but if we don't handle them, they end
    // up in the global event queue, which is messing to handle.
    seat.quick_assign(|_main, event, _c| match event {
        wayland_client::protocol::wl_seat::Event::Capabilities { capabilities } => {
            eprint!("Capabilities: {:?}", capabilities)
        }
        wayland_client::protocol::wl_seat::Event::Name { name } => {
            eprint!("Seat name: {}", name)
        }
        _ => unreachable!(),
    });

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

    let data_device = manager.get_data_device(&seat); // <<-- the thing I need to use.
    data_device.quick_assign(|_main, ev, _dispatch_data| match ev {
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
            // CLIPBOARD selection

            // This is sent AFTER the offers, and indicates that all the types and stuff are set.
            // The id is that of the objet gotten via DataOffer.

            // The id can be null, which just expires the previous offer.
            eprintln!("selection: {:?}", id);
        }
        zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
            // PRIMARY selection
            // Same as above
            eprintln!("primary: {:?}", id)
        }
        zwlr_data_control_device_v1::Event::Finished => eprintln!("Finished"),
        _ => unreachable!(),
    });

    println!("Data device: {:?}", data_device);

    loop {
        // The dispatch() method returns once it has received some events to dispatch
        // and have emptied the wayland socket from its pending messages, so it needs
        // to be called in a loop. If this method returns an error, your connection to
        // the wayland server is very likely dead. See its documentation for more details.
        event_queue
            // There's a bug, and this event handler won't work anyway.
            // Register explicitly handlers everywhere.
            .dispatch(&mut (), |raw_event, _main, _dispatch_data| {
                eprintln!(
                    "Unhandled / unexpected event: '{}.{}'.",
                    raw_event.interface, raw_event.name,
                );
            })
            .expect("An error occurred during event dispatching!");
    }
}
