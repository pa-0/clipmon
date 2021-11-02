use crate::DataOffer;
use crate::LoopData;
use crate::MimeTypes;
use crate::Selection;

use calloop::generic::Generic;
use calloop::Interest;
use calloop::LoopHandle;
use calloop::Mode;
use calloop::PostAction;
use os_pipe::PipeReader;
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
            let selection_data = loop_data.get_selection_data(*selection);

            let inner = match selection_data {
                Some(data) => data,
                None => {
                    eprintln!("No data for {:?}!", selection);
                    return;
                }
            };
            let inner = inner.borrow();

            let typed_data = match inner.get(&mime_type) {
                Some(data) => match data {
                    Some(inner) => inner,
                    None => {
                        eprintln!(
                            "Data is missing for mime_type: {:?},{:?}!",
                            selection, mime_type
                        );
                        return;
                    }
                },
                None => {
                    eprintln!(
                        "Client requested unavailable mime_type: {:?},{:?}!",
                        selection, mime_type
                    );
                    return;
                }
            };

            let r = file.write(typed_data);

            match r {
                Ok(bytes) => println!(
                    "zwlr_data_control_source_v1@{:?} - Sent {} bytes.",
                    main.as_ref().id(),
                    bytes
                ),
                Err(err) => println!("Error sending selection: {:?}", err),
            }
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
    // XXX: Does it make more sense to pass the selection data itself?
    data_source.as_ref().user_data().set(move || *selection);
    data_source.quick_assign(handle_source_event);

    for (mime_type, _) in mime_types.borrow().iter() {
        data_source.offer(mime_type.clone());
    }

    loop_data.take_selection(*selection, mime_types, &data_source); // Race condition??
}

fn handle_pipe_event(
    reader: &mut PipeReader,
    mime_type: &str,
    mime_types: &MimeTypes,
    loop_data: &mut LoopData,
    selection: &Selection,
    data_offer_id: u32,
) -> Result<PostAction, std::io::Error> {
    let mut reader = std::io::BufReader::new(reader);
    let mut buf = Vec::<u8>::new();
    let len = reader.read_to_end(&mut buf)?;

    println!(
        "zwlr_data_control_offer_v1@{:?} - Finished reading {}, {:?} bytes.",
        data_offer_id, mime_type, len
    );

    // Save the read value into our user data.
    mime_types
        .borrow_mut()
        .insert(mime_type.to_string(), Some(buf));

    // Check if we've already copied all mime types...
    if !mime_types.borrow().iter().any(|(_, value)| value.is_none()) {
        if loop_data.is_selection_owned_by_client(*selection, data_offer_id) {
            create_data_source(loop_data, mime_types, selection);
        } else {
            println!(
                "{:?} - No longer owns {:?} selection, bailing",
                data_offer_id, selection
            );
            // TODO: Do i need to drop mime-data?
        }
    }

    // Given that we've read all the data, no need to continue
    // having this source in the event loop:
    Result::Ok(PostAction::Remove)
}

pub fn read_offer(
    data_offer: &ZwlrDataControlOfferV1,
    handle: &LoopHandle<LoopData>,
    user_data: &DataOffer,
) {
    // TODO: I might want to be smart about some types here.
    // "UTF8_STRING" and "text/plain;charset=utf-8" should be the same, so
    // copying just one might suffice.
    for (mime_type, _) in user_data.mime_types.borrow().iter() {
        let (reader, writer) = match os_pipe::pipe() {
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
        let selection = user_data.selection.borrow().expect("can borrow selection from user_data");
        let id = data_offer.as_ref().id();

        match handle.insert_source(source, move |_event, reader, data| {
            handle_pipe_event(reader, &mime_type, &mime_types, data, &selection, id)
        }) {
            Ok(_) => {}
            Err(err) => println!("Error setting handler for pipe: {:?}", err),
        }
    }
}
