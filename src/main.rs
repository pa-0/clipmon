use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::Display;
use wayland_client::GlobalManager;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;

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
    println!("Have a seat: {:?}", seat);

    event_queue
        .sync_roundtrip(&mut (), |raw_event, _, _| {
            // This is usually the compositor telling us about its capabilities.
            // I don't think we need to do anything about this.
            println!("Got capabilities: {:?}", raw_event)
        })
        .unwrap();

    let manager = match globals.instantiate_exact::<ZwlrDataControlManagerV1>(1) {
        Err(err) => {
            println!("Compositor doesn't support wlr-data-control-unstable-v1.");
            panic!("{}", err);
        }
        Ok(res) => res,
    };

    let data_device = manager.get_data_device(&seat); // <<-- the thing I need to use.
    println!("Got the data device: {:?}", data_device);

    // Crash happens after this line (which just sends the above and gets a reply) ------------
    event_queue
        .sync_roundtrip(&mut (), |_, _, _| unreachable!())
        .unwrap();
    // Crash happens above this line ------------

    unreachable!("This line is never reached :(");

    // event_queue
    //     .sync_roundtrip(&mut (), |raw_event, _, _| {
    //         println!("Asked for manager: {:?}", raw_event)
    //     })
    //     .unwrap();
    //
    // println!("01");
    //
    // manager.create_data_source(); // <<-- I won't need this until I work as a source.
    //
    // event_queue
    //     .sync_roundtrip(&mut (), |raw_event, _, _| {
    //         println!("10{}", raw_event.interface)
    //     })
    //     .unwrap();
    // println!("11");
    // // https://docs.rs/wayland-client/0.29.0/wayland_client/struct.EventQueue.html
    // loop {
    //     // The dispatch() method returns once it has received some events to dispatch
    //     // and have emptied the wayland socket from its pending messages, so it needs
    //     // to be called in a loop. If this method returns an error, your connection to
    //     // the wayland server is very likely dead. See its documentation for more details.
    //     event_queue
    //         // There's a bug, and this event handler won't work anyway.
    //         // Register explicitly handlers everywhere.
    //         .dispatch(&mut (), |_, _, _| unreachable!())
    //         .expect("An error occurred during event dispatching!");
    // }
}
