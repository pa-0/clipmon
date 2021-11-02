use crate::DataOffer;
use crate::LoopData;
use crate::MimeTypes;
use crate::Selection;

use calloop::generic::Generic;
use calloop::Interest;
use calloop::LoopHandle;
use calloop::Mode;
use calloop::PostAction;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::rc::Rc;
use wayland_client::DispatchData;
use wayland_client::Main;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1::ZwlrDataControlSourceV1;

fn handle_source_event(
    main: Main<ZwlrDataControlSourceV1>,
    event: zwlr_data_control_source_v1::Event,
    mut data: DispatchData,
) {
    let loop_data = data
        .get::<LoopData>()
        .expect("dispatch data is of type LoopData");
    let selection = main
        .as_ref()
        .user_data()
        .get::<Selection>()
        .expect("user_data is of type Selection");

    match event {
        zwlr_data_control_source_v1::Event::Send { mime_type, fd } => {
            let mut file = unsafe { File::from_raw_fd(fd) };

            let mime_types = match loop_data.get_selection_data(*selection) {
                Some(data) => data,
                None => {
                    eprintln!("No data for {:?}!", selection);
                    return;
                }
            };
            let inner = mime_types.borrow();

            let selection_data = match inner.get(&mime_type) {
                Some(data) => data,
                None => {
                    eprintln!(
                        "Client requested unavailable mime_type: {:?},{:?}!",
                        selection, mime_type
                    );
                    return;
                }
            };

            // Triggers a notification indicating that a client has pasted.
            loop_data.notification.ping();

            match file.write(&selection_data.data.borrow()) {
                Ok(bytes) => println!(
                    "zwlr_data_control_source_v1@{:?} - Sent {} bytes.",
                    main.as_ref().id(),
                    bytes
                ),
                Err(err) => println!("Error sending selection: {:?}", err),
            };
        }
        zwlr_data_control_source_v1::Event::Cancelled {} => {
            loop_data.selection_lost(*selection);
            main.destroy();
        }
        _ => unreachable!(),
    }
}

fn create_data_source(loop_data: &mut LoopData, mime_types: &MimeTypes, selection: &Selection) {
    let data_source = loop_data.manager.create_data_source();
    // Pass the selection since this source needs to know what to send:
    data_source.as_ref().user_data().set(move || *selection);
    data_source.quick_assign(handle_source_event);

    for (mime_type, _) in mime_types.borrow().iter() {
        data_source.offer(mime_type.clone());
    }

    loop_data.take_selection(*selection, mime_types, &data_source); // Race condition??
}

fn handle_pipe_event(
    reader: &mut File,
    mime_type: &str,
    mime_types: &MimeTypes,
    loop_data: &mut LoopData,
    selection: &Selection,
    data_offer_id: u32,
) -> Result<PostAction, std::io::Error> {
    // TODO: extract all the "read to Vec<u8>" logic into a reusable helper.

    let mut reader = std::io::BufReader::new(reader);
    let mut_mime_types = &mime_types.borrow();
    let selection_data = mut_mime_types
        .get(mime_type)
        .expect("mime_types contains the read mime_type entry");

    loop {
        let mut buf = [0; 32];
        let len = match reader.read(&mut buf) {
            Ok(len) => len,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(PostAction::Continue);
                } else {
                    return Err(e);
                }
            }
        };

        if len == 0 {
            break;
        }

        println!(
            "zwlr_data_control_offer_v1@{:?} - Read {}, {:?} bytes.",
            data_offer_id, mime_type, len
        );

        selection_data
            .data
            .borrow_mut()
            .extend_from_slice(&buf[0..len]);
    }

    println!(
        "zwlr_data_control_offer_v1@{:?} - Finished reading {}, total {:?} bytes.",
        data_offer_id,
        mime_type,
        selection_data.data.borrow().len()
    );

    selection_data.is_complete.replace(true);

    // Check if we've already copied all mime types...
    if !mime_types
        .borrow()
        .iter()
        .any(|(_, value)| !*value.is_complete.borrow())
    {
        if loop_data.is_selection_owned_by_client(*selection, data_offer_id) {
            create_data_source(loop_data, mime_types, selection);
        } else {
            println!(
                "{:?} - No longer owns {:?} selection, bailing",
                data_offer_id, selection
            );
        }
    }

    // Given that we've read all the data, no need to continue
    // having this source in the event loop:
    Result::Ok(PostAction::Remove)
}

fn create_pipes() -> Result<(File, File), std::io::Error> {
    let mut fds: [libc::c_int; 2] = [0; 2];
    let res = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) };
    if res != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let reader = unsafe { File::from_raw_fd(fds[0]) };
    let writer = unsafe { File::from_raw_fd(fds[1]) };
    Ok((reader, writer))
}

pub fn read_offer(
    data_offer: &ZwlrDataControlOfferV1,
    handle: &LoopHandle<LoopData>,
    user_data: &DataOffer,
) {
    // TODO: I might want to be smart about some types here.
    // "UTF8_STRING" and "text/plain;charset=utf-8" should be the same, so
    // copying just one might suffice.
    for (mime_type, _selection_data) in user_data.mime_types.borrow().iter() {
        let (reader, writer) = match create_pipes() {
            Ok((reader, writer)) => (reader, writer),
            Err(err) => {
                eprintln!("Could not open pipe to read data: {:?}", err);
                continue;
            }
        };
        data_offer.receive(mime_type.clone(), writer.as_raw_fd());
        drop(writer); // We won't write anything, the selection client will.

        let source = Generic::new(reader, Interest::READ, Mode::Edge);
        let mime_type = mime_type.clone();
        let mime_types = Rc::clone(&user_data.mime_types);
        let selection = user_data
            .selection
            .borrow()
            .expect("can borrow selection from user_data");
        let id = data_offer.as_ref().id();

        handle
            .insert_source(source, move |_event, reader, loop_data| {
                handle_pipe_event(reader, &mime_type, &mime_types, loop_data, &selection, id)
            })
            .expect("handler for pipe event is set");
    }
}
