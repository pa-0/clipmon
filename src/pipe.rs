use crate::ControlOfferUserData;
use calloop::generic::Generic;
use calloop::Interest;
use calloop::LoopHandle;
use calloop::LoopSignal;
use calloop::Mode;
use calloop::PostAction;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

pub fn read_offer(data_offer: &ZwlrDataControlOfferV1, handle: &LoopHandle<LoopSignal>) {
    // let user_data = match data_offer
    //     .as_ref()
    //     .user_data()
    //     .get::<ControlOfferUserData>()
    // {
    //     Some(data) => data,
    //     None => {
    //         println!("We're missing data for this offer! o.O");
    //         return;
    //     }
    // };
    let user_data = data_offer
        .as_ref()
        .user_data()
        .get::<ControlOfferUserData>()
        .unwrap();

    for (mime_type, _) in user_data.mime_types.borrow().iter() {
        println!("receiving type:{}, {:?}", mime_type, user_data);
        let (r, w) = os_pipe::pipe().unwrap();
        data_offer.receive(mime_type.to_string(), w.as_raw_fd());
        drop(w);

        let source = Generic::new(r, Interest::READ, Mode::Edge);
        let mime_type = mime_type.clone();
        let id = data_offer.as_ref().id().clone();
        let mime_types = Rc::clone(&user_data.mime_types);

        match handle.insert_source(source, move |_event, reader, _data| {
            let mut reader = std::io::BufReader::new(reader);
            let mut buf = Vec::<u8>::new();
            let len = reader.read_to_end(&mut buf)?;

            println!(
                "read. data_offer: {:?}/{}: {:?} {:?}, {:?}",
                id, mime_type, len, buf, reader
            );

            // Save the read value into our user data.
            mime_types.borrow_mut().insert(mime_type.clone(), Some(buf));

            // Check if we've already copied all mime types...
            if mime_types.borrow().iter().any(|(_, value)| {
                return value.is_none();
            }) {
                // TODO: All mime types copied, grab the clipboard now.
            }

            // Given that we've read all the data, no need to continue
            // having this source in the event loop:
            return Result::Ok(PostAction::Remove);
        }) {
            Ok(_) => {}
            Err(err) => println!("Error reading selection: {:?}", err),
        }
    }
}
